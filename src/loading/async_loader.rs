use macroquad::prelude::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::texture_pipeline::{EmbeddedMetadata, ImageInfo, Pipeline};

#[derive(Clone)]
pub struct LoadedImage {
    pub texture: Texture2D,
    pub info: ImageInfo,
    pub path: std::path::PathBuf,
}

pub struct AsyncImageLoader {
    completed_images: Arc<Mutex<HashMap<String, Result<LoadedImageResult, String>>>>,
    max_updates_per_frame: usize,
    cancel_flag: Arc<AtomicBool>, // Atomic flag for cancellation
}

struct LoadedImageResult {
    parsed_image: Image,
    info: ImageInfo,
    source_path: std::path::PathBuf,
}

impl Default for AsyncImageLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncImageLoader {
    pub fn new() -> Self {
        Self {
            completed_images: Arc::new(Mutex::new(HashMap::new())),
            max_updates_per_frame: 1, // Only process 1 texture per frame to keep UI responsive
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_loading_batch(&mut self, metadata_list: Vec<EmbeddedMetadata>) {
        log::info!(
            "🚀 Starting batch loading for {} images",
            metadata_list.len()
        );

        // Reset cancellation flag for new batch
        self.cancel_flag.store(false, Ordering::Relaxed);

        let completed_images = self.completed_images.clone();
        let cancel_flag = self.cancel_flag.clone();

        rayon::spawn(move || {
            metadata_list.into_par_iter().for_each(|metadata| {
                // Check for cancellation before processing each image
                if cancel_flag.load(Ordering::Relaxed) {
                    log::debug!("🚫 Cancellation requested, skipping image load");
                    return;
                }

                let key = format!("{}:{}", metadata.source_path.display(), metadata.name);
                let result = Self::load_single_image_with_hint(metadata);

                // Check for cancellation before storing result
                if cancel_flag.load(Ordering::Relaxed) {
                    log::debug!("🚫 Cancellation requested, not storing result for {key}");
                    return;
                }

                if let Ok(mut completed) = completed_images.lock() {
                    match &result {
                        Ok(_) => log::info!("✅ Rayon completed successfully: {key}"),
                        Err(e) => log::warn!("⚠️ Rayon skipping file: {key}: {e}"),
                    }
                    completed.insert(key, result);
                } else {
                    log::error!("🔒 Failed to acquire lock for completed_images: {key}");
                }
            });

            if cancel_flag.load(Ordering::Relaxed) {
                log::info!("🚫 Rayon batch loading was cancelled");
            }
        });
    }

    /// NEW: Direct hint-based loading - NO RE-PARSING of containers!
    /// This follows the refactoring plan exactly
    fn load_single_image_with_hint(
        metadata: EmbeddedMetadata,
    ) -> Result<LoadedImageResult, String> {
        let key = format!("{}:{}", metadata.source_path.display(), metadata.name);

        let pipeline = Pipeline::new();

        // Use the hint system for direct access - NO container re-parsing!
        let loaded_data = pipeline.metadata_to_loaded_data(&metadata).map_err(|e| {
            let error_msg = format!("Failed to load image data using hint: {e}");
            log::error!("Failed to load {key}: {e}");
            error_msg
        })?;

        // Parse the loaded data to macroquad format
        let (macroquad_image, info) = pipeline.parse_image_data(&loaded_data).map_err(|e| {
            let error_msg = format!("Parse error: {e}");
            log::warn!("⚠️ Skipping texture due to parse error {key}: {e}");
            error_msg
        })?;

        Ok(LoadedImageResult {
            parsed_image: macroquad_image,
            info,
            source_path: metadata.source_path.clone(),
        })
    }

    pub fn update(&mut self) -> Vec<(String, Result<LoadedImage, String>)> {
        let mut completed = Vec::new();
        let mut processed_count = 0;

        if let Ok(mut completed_images) = self.completed_images.try_lock() {
            let keys_to_process: Vec<_> = completed_images.keys().cloned().collect();

            for key in keys_to_process {
                if processed_count >= self.max_updates_per_frame {
                    break;
                }

                if let Some(result) = completed_images.remove(&key) {
                    let final_result = match result {
                        Ok(loaded_result) => {
                            let texture = Texture2D::from_image(&loaded_result.parsed_image);
                            // Start with linear filtering as default, will be changed at render time
                            texture.set_filter(FilterMode::Linear);

                            Ok(LoadedImage {
                                texture,
                                info: loaded_result.info,
                                path: loaded_result.source_path,
                            })
                        }
                        Err(error) => Err(error),
                    };

                    completed.push((key, final_result));
                    processed_count += 1;
                }
            }
        }

        completed
    }

    /// Cancel all ongoing loading operations and clear completed results
    pub fn cancel_all(&mut self) {
        log::info!("🚫 Cancelling all async loading operations");

        // Set cancellation flag to stop ongoing Rayon tasks
        self.cancel_flag.store(true, Ordering::Relaxed);

        // Clear completed results
        if let Ok(mut completed) = self.completed_images.lock() {
            let cleared_count = completed.len();
            completed.clear();
            if cleared_count > 0 {
                log::info!("🧹 Cleared {cleared_count} completed loading results");
            }
        }
    }

    /// Check if cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }
}
