use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;

use crate::texture_pipeline::Pipeline;
use crate::types::{GTexViewerApp, ImageSlot, ImageState};
use macroquad::prelude::Vec2;

impl GTexViewerApp {
    fn collect_image_files_recursively(path: &PathBuf) -> Vec<PathBuf> {
        let mut image_files = Vec::new();

        if let Ok(metadata) = fs::metadata(path) {
            if metadata.is_file() {
                // Check if this individual file is supported using lightweight format detection
                let pipeline = Pipeline::new();
                if let Some(source) = pipeline.source_registry().find_source(path)
                    && source.can_load_path(path).unwrap_or(false)
                {
                    image_files.push(path.clone());
                }
            } else if metadata.is_dir() {
                // Recursively traverse directory
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        // Recursively collect from subdirectories and files
                        image_files.extend(Self::collect_image_files_recursively(&entry_path));
                    }
                }
            }
        }

        image_files
    }

    /// Cancel all ongoing operations and clean state for fresh start
    pub fn cancel_all_loading(&mut self) {
        log::info!("🚫 Cancelling all loading operations");

        // Set cancellation flags
        self.metadata_cancel_flag.store(true, Ordering::Relaxed);
        self.async_loader.cancel_all();

        // Clear all state
        self.image_slots.clear();
        self.metadata_receivers.clear();
        self.pending_metadata.clear();

        // Reset loading state
        self.is_loading = false;
        self.loading_completed_once = false;
        self.layout_needs_update = true;

        log::info!("🧹 All loading operations cancelled and state cleared");
    }
    pub fn handle_file_drops(&mut self) {
        // Get dropped files from macroquad
        use macroquad::prelude::*;
        let dropped_files = get_dropped_files();

        if !dropped_files.is_empty() {
            let dropped_paths: Vec<PathBuf> = dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();

            // Recursively collect all image files from dropped paths (files and directories)
            let mut all_paths = Vec::new();
            for path in dropped_paths {
                all_paths.extend(Self::collect_image_files_recursively(&path));
            }

            if !all_paths.is_empty() {
                // Cancel all ongoing operations first
                self.cancel_all_loading();

                // Reset camera view position to show new images
                self.camera = macroquad::prelude::Camera2D::default();

                self.load_images(all_paths);

                // Start burst rendering to ensure file drop UI updates are fully drawn
                self.start_burst_rendering(std::time::Duration::from_secs(1));
            }
        }
    }

    pub fn load_initial_file_if_needed(&mut self) {
        if let Some(path) = self.initial_file_path.take() {
            self.load_images(vec![path]);
            // Trigger redraw when initial file starts loading
            macroquad::miniquad::window::schedule_update();
        }
    }

    pub fn load_images(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        // Reset cancellation flag for new loading session
        self.metadata_cancel_flag.store(false, Ordering::Relaxed);

        self.is_loading = true;
        self.loading_completed_once = false; // Reset completion flag for new loading session

        // Paths are already filtered by collect_image_files_recursively
        let supported_paths = paths;

        // Skip initial placeholder creation - wait for proper metadata with hints
        // This ensures we always have proper EmbeddedMetadata with working hints

        // Trigger immediate layout update so placeholders are visible
        self.layout_needs_update = true;
        self.newly_loaded = true; // Force layout recalculation

        // Phase 1: Start metadata extraction in batches to avoid overwhelming the system
        // For 63 files, spawning 63 threads at once can block the UI
        let batch_size = 8; // Limit concurrent metadata extraction threads

        log::info!(
            "Starting metadata extraction for {} supported files in batches",
            supported_paths.len()
        );

        for (batch_index, paths_batch) in supported_paths.chunks(batch_size).enumerate() {
            let (batch_sender, batch_receiver) = mpsc::channel();
            self.metadata_receivers.push(batch_receiver);

            log::debug!(
                "Starting batch {} with {} files",
                batch_index,
                paths_batch.len()
            );

            let paths_batch = paths_batch.to_vec();
            let cancel_flag = self.metadata_cancel_flag.clone();
            thread::spawn(move || {
                log::debug!(
                    "Batch {} thread started with {} paths",
                    batch_index,
                    paths_batch.len()
                );

                // Check for early cancellation
                if cancel_flag.load(Ordering::Relaxed) {
                    log::debug!("🚫 Batch {batch_index} cancelled before processing");
                    return;
                }

                let pipeline = Pipeline::new();

                // Use queue-based recursive processing following proper pipeline design
                let embedded_metadata =
                    pipeline.extract_all_metadata_recursive(paths_batch.clone());

                // Check for cancellation before sending results
                if cancel_flag.load(Ordering::Relaxed) {
                    log::debug!("🚫 Batch {batch_index} cancelled after processing");
                    return;
                }

                // Use EmbeddedMetadata directly - no conversion needed!
                let batch_results = embedded_metadata;

                let mut any_sent = false; // We'll handle this after processing

                // Send successful results as a batch
                if !batch_results.is_empty() {
                    log::debug!(
                        "Batch {} sending {} metadata results",
                        batch_index,
                        batch_results.len()
                    );
                    let _ = batch_sender.send(Ok(batch_results));
                    any_sent = true;
                }

                // Ensure every batch thread sends at least one message to signal completion
                // Even if no files could be processed, send an empty batch
                if !any_sent {
                    log::debug!("Batch {batch_index} sending empty completion signal");
                    let _ = batch_sender.send(Ok(Vec::new()));
                }

                log::debug!("Batch {batch_index} thread completed");
            });
        }
    }

    pub fn update_async_loading(&mut self) {
        // Check for completed images from Rayon
        let completed = self.async_loader.update();
        let mut failed_keys = Vec::new();

        for (key, result) in completed {
            // Find the corresponding slot and update it
            if let Some(slot) = self.find_slot_by_key(&key) {
                match result {
                    Ok(loaded_image) => {
                        log::info!("Successfully loaded image: {key}");
                        slot.state = ImageState::Loaded {
                            image: loaded_image,
                        };
                        // Don't trigger layout recalculation - just replace placeholder with loaded image

                        // For single images, trigger auto-centering
                        if self.image_slots.len() == 1 {
                            self.newly_loaded = true;
                        }

                        // Trigger redraw when image loads successfully
                        macroquad::miniquad::window::schedule_update();
                    }
                    Err(error) => {
                        log::warn!("Removing placeholder for skipped image {key}: {error}");
                        failed_keys.push(key);
                    }
                }
            } else {
                log::warn!("Could not find slot for key: {key}");
            }
        }

        // Remove slots for failed/skipped images
        if !failed_keys.is_empty() {
            self.image_slots.retain(|slot| {
                let slot_key = match &slot.state {
                    ImageState::Placeholder {
                        original_metadata, ..
                    } => Some(format!(
                        "{}:{}",
                        original_metadata.source_path.display(),
                        original_metadata.name
                    )),
                    ImageState::Loaded { image } => Some(format!(
                        "{}:{}",
                        image.path.display(),
                        image
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                    )),
                    ImageState::Failed { metadata, .. } => metadata
                        .as_ref()
                        .map(|m| format!("{}:{}", m.source_path.display(), m.name)),
                };

                // Keep slots that don't match any failed key
                !failed_keys
                    .iter()
                    .any(|failed_key| slot_key.as_deref() == Some(failed_key))
            });

            if !failed_keys.is_empty() {
                self.layout_needs_update = true;
                macroquad::miniquad::window::schedule_update();
            }
        }

        // Check if all loading is complete
        if self.is_loading && !self.loading_completed_once && self.metadata_receivers.is_empty() {
            // Check if we have any placeholder states left
            let still_loading = self
                .image_slots
                .iter()
                .any(|slot| matches!(slot.state, ImageState::Placeholder { .. }));

            if !still_loading {
                let has_loaded = self
                    .image_slots
                    .iter()
                    .any(|slot| matches!(slot.state, ImageState::Loaded { .. }));

                if has_loaded {
                    // We have successfully loaded images
                    self.is_loading = false;
                    self.loading_completed_once = true;
                    self.newly_loaded = true; // Mark for auto-fit
                } else if self.image_slots.is_empty() {
                    // No images left (all failed/skipped) - reset to initial state
                    self.is_loading = false;
                    self.loading_completed_once = false;
                    self.newly_loaded = false;
                }
            }
        }
    }

    pub fn find_slot_by_key(&mut self, key: &str) -> Option<&mut ImageSlot> {
        self.image_slots.iter_mut().find(|slot| {
            let slot_key = match &slot.state {
                ImageState::Placeholder {
                    original_metadata, ..
                } => Some(format!(
                    "{}:{}",
                    original_metadata.source_path.display(),
                    original_metadata.name
                )),
                _ => None,
            };

            slot_key.as_ref().is_some_and(|k| k == key)
        })
    }

    pub fn check_metadata_results(&mut self) {
        let mut completed_receivers = Vec::new();
        let mut new_metadata_list = Vec::new();

        // Check if current loading was cancelled - if so, ignore all results
        if self.metadata_cancel_flag.load(Ordering::Relaxed) {
            log::debug!("🚫 Loading cancelled, clearing all metadata receivers");
            self.metadata_receivers.clear();
            self.pending_metadata.clear();
            return;
        }

        // Check all metadata receivers for completed extraction
        for (index, receiver) in self.metadata_receivers.iter().enumerate() {
            let mut receiver_completed = false;
            let mut messages_received = 0;

            // Drain ALL messages from this receiver
            while let Ok(result) = receiver.try_recv() {
                receiver_completed = true;
                messages_received += 1;

                match result {
                    Ok(metadata_list) => {
                        log::debug!(
                            "Receiver {index} got {} metadata items",
                            metadata_list.len()
                        );
                        if !metadata_list.is_empty() {
                            new_metadata_list.extend(metadata_list);
                            self.layout_needs_update = true;
                        }
                    }
                    Err((path, error)) => {
                        log::error!("Failed to extract metadata from {path:?}: {error}");

                        // Create a failed slot only for actual errors (not unsupported formats)
                        let slot = ImageSlot {
                            state: ImageState::Failed {
                                metadata: None,
                                error: error.clone(),
                            },
                            position: Vec2::ZERO,
                            size: Vec2::ZERO,
                        };
                        self.image_slots.push(slot);
                        self.layout_needs_update = true;
                    }
                }
            }

            // Mark receiver as completed if we processed any messages
            if receiver_completed {
                log::debug!("Receiver {index} completed with {messages_received} messages");
                completed_receivers.push(index);
            }
        }

        // Accumulate metadata until all arrive
        if !new_metadata_list.is_empty() {
            self.pending_metadata.extend(new_metadata_list);
        }

        // Check if all metadata extraction is complete
        let remaining_receivers = self.metadata_receivers.len() - completed_receivers.len();
        let all_metadata_complete = remaining_receivers == 0;

        if all_metadata_complete && !self.pending_metadata.is_empty() {
            // Clear any existing placeholder slots and create new ones with adjusted dimensions
            self.image_slots.clear();

            // Create placeholder slots with both original and adjusted dimensions
            for metadata in &self.pending_metadata {
                let adjusted_metadata = Self::adjust_metadata_for_layout(metadata);

                let slot = ImageSlot {
                    state: ImageState::Placeholder {
                        original_metadata: metadata.clone(),
                        layout_metadata: adjusted_metadata,
                    },
                    position: Vec2::ZERO, // Layout will calculate these
                    size: Vec2::ZERO,     // Layout will calculate these
                };
                self.image_slots.push(slot);
            }

            // Calculate layout with adjusted dimensions
            self.layout_needs_update = true;

            // Start burst rendering when placeholders are created to ensure loading UI is drawn
            self.start_burst_rendering(std::time::Duration::from_millis(500));

            // Start async loading with original metadata (not adjusted)
            let original_metadata = self.pending_metadata.clone();
            self.async_loader.start_loading_batch(original_metadata);

            // Clear pending metadata since we've processed it
            self.pending_metadata.clear();
        }

        // Remove completed receivers (in reverse order to maintain indices)
        for &index in completed_receivers.iter().rev() {
            self.metadata_receivers.remove(index);
        }

        // Note: macroquad handles frame timing automatically
    }
}
