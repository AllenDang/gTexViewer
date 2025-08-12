use anyhow::Result;
use macroquad::prelude::*;
use std::path::{Path, PathBuf};

// Sub-modules
pub mod hint;
pub mod parsers;
pub mod registry;
pub mod source;
pub mod sources;

// Re-export key types for external use
pub use hint::{EmbeddedHint, EmbeddedMetadata, FbxHint, FileHint, GlbHint, ZipHint};
pub use registry::SourceRegistry;
pub use source::{BufReadSeek, Source};

use sources::{FbxSource, GlbSource, ImageSource, ZipSource};

/// Raw image data loaded by a source with pre-detected format and dimensions
#[derive(Debug, Clone)]
pub struct LoadedImageData {
    pub name: String,
    pub data: Vec<u8>,
    pub file_size: usize,
    pub source_file: PathBuf,
    pub format: imagesize::ImageType, // Pre-detected format (PNG, JPEG, etc.)
    pub width: usize,                 // Pre-detected width
    pub height: usize,                // Pre-detected height
}

/// Processed image information after parsing
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub color_space: String,
}

/// Trait for parsing raw image data into macroquad-compatible format
pub trait ImageDataParser: Send + Sync {
    fn can_parse(&self, data: &LoadedImageData) -> bool;
    fn parse(&self, data: &LoadedImageData) -> Result<(Image, ImageInfo)>;
}

/// Main texture loading pipeline - replaces the old TextureLoader entirely
///
/// This is the core of the new architecture according to the refactoring plan:
/// Input Path → Source Detection → Metadata Phase → UI Phase → Async Load Phase
///
/// According to refact_pipeline.md: Simple pipeline coordinator without caching
pub struct Pipeline {
    source_registry: SourceRegistry,
    parsers: Vec<Box<dyn ImageDataParser>>,
}

impl Pipeline {
    /// Create a new pipeline with all available sources and parsers
    pub fn new() -> Self {
        // Create source registry with all available sources
        let mut source_registry = SourceRegistry::new();

        // Add sources in priority order:
        // 1. Container sources (GLB, FBX, ZIP) - handle specific formats first
        source_registry.add_source(Box::new(GlbSource));
        source_registry.add_source(Box::new(FbxSource));
        source_registry.add_source(Box::new(ZipSource));

        // 2. Universal image source - handles all remaining image formats via imagesize
        source_registry.add_source(Box::new(ImageSource));

        // Register data parsers
        let parsers: Vec<Box<dyn ImageDataParser>> = vec![
            Box::new(parsers::StandardFormat),
            Box::new(parsers::Ktx2Format),
            Box::new(parsers::CompressedFormat),
        ];

        Self {
            source_registry,
            parsers,
        }
    }

    /// Phase 1: Source Detection + Metadata Extraction
    /// Fast metadata extraction for a single path using SourceRegistry
    /// Parse container ONCE and create hints with direct access information
    pub fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>> {
        if let Some(source) = self.source_registry.find_source(path) {
            return source.extract_metadata(path);
        }

        // Return empty vec for unsupported formats instead of error
        log::debug!(
            "Skipping unsupported file format: {} ({})",
            path.display(),
            path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("unknown")
        );
        Ok(Vec::new())
    }

    /// Phase 1: Parallel Source Detection + Metadata Extraction
    /// High-performance batch processing using SourceRegistry's parallel capabilities
    pub fn extract_all_metadata_parallel(&self, paths: Vec<PathBuf>) -> Vec<EmbeddedMetadata> {
        self.source_registry.extract_all_metadata_parallel(paths)
    }

    /// Phase 1: Fast Metadata Extraction with Smart Placeholder Creation
    /// Gets initial metadata quickly, with accurate counts for containers  
    pub fn extract_all_metadata_fast(&self, paths: Vec<PathBuf>) -> Vec<EmbeddedMetadata> {
        let initial_metadata = self.extract_all_metadata_parallel(paths);

        log::info!(
            "Fast metadata extraction completed: {} items found",
            initial_metadata.len()
        );

        initial_metadata
    }

    /// Phase 1: Recursive Container Processing using queue-based pipeline approach
    /// Follows proper pipeline design by queuing containers for reprocessing
    pub fn extract_all_metadata_recursive(&self, paths: Vec<PathBuf>) -> Vec<EmbeddedMetadata> {
        let mut processing_queue: std::collections::VecDeque<EmbeddedMetadata> = Vec::new().into();
        let mut final_metadata = Vec::new();

        // Start with initial file paths - convert to metadata entries for uniform processing
        for path in paths {
            if let Some(source) = self.source_registry.find_source(&path) {
                match source.extract_metadata(&path) {
                    Ok(metadata_list) => {
                        for meta in metadata_list {
                            processing_queue.push_back(meta);
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to extract initial metadata from {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        // Process queue - containers push new entries to back of queue (pipeline pattern)
        while let Some(meta) = processing_queue.pop_front() {
            if let Some(header) = meta.embedded_hint.header_bytes() {
                let header_vec = header.to_vec(); // Copy to avoid borrow issues
                // Try direct image detection first
                if let Ok(detected_format) = imagesize::image_type(&header_vec) {
                    // This is a direct image - finalize and add to results
                    let mut updated_meta = meta;
                    updated_meta.format = detected_format;
                    if let Ok(size) = imagesize::blob_size(&header_vec) {
                        updated_meta.width = size.width;
                        updated_meta.height = size.height;
                    }
                    final_metadata.push(updated_meta);
                } else {
                    // Check if it's a container format
                    let mut header_cursor = std::io::Cursor::new(&header_vec);
                    if let Some(container_source) = self
                        .source_registry
                        .find_source_for_reader(&mut header_cursor)
                    {
                        // This is a container - expand and push entries to back of queue
                        log::debug!(
                            "Found container {}, expanding and queuing entries",
                            meta.name
                        );

                        // Load container data and extract its contents
                        if let Some(parent_source) =
                            self.source_registry.find_source(&meta.source_path)
                            && let Ok(container_data) =
                                parent_source.load_bytes(meta.embedded_hint.as_ref())
                        {
                            let mut container_cursor = std::io::Cursor::new(&container_data);
                            if let Ok(expanded_metadata) = container_source
                                .extract_metadata_from_reader(
                                    &mut container_cursor,
                                    &meta.name,
                                    &meta.source_path,
                                )
                            {
                                // Push expanded entries to back of queue for processing
                                for expanded_meta in expanded_metadata {
                                    processing_queue.push_back(expanded_meta);
                                }
                                log::info!(
                                    "Container {} expanded, {} entries queued",
                                    meta.name,
                                    processing_queue.len()
                                );
                            }
                        }
                    } else {
                        // Unknown format - skip
                        log::debug!("Skipping unknown format: {}", meta.name);
                    }
                }
            } else {
                // No header bytes - add directly to results
                final_metadata.push(meta);
            }
        }

        final_metadata
    }

    /// Phase 4: Async Load Phase - Load raw image data using hint
    /// Uses the hint system to efficiently load specific content using direct access
    pub fn load_bytes(&self, metadata: &EmbeddedMetadata) -> Result<Vec<u8>> {
        // Check if the hint has embedded data (for nested containers)
        if let Some(glb_hint) = metadata
            .embedded_hint
            .as_any()
            .downcast_ref::<crate::texture_pipeline::GlbHint>()
            && let Some(ref texture_data) = glb_hint.texture_data
        {
            return Ok(texture_data.clone());
        }

        if let Some(fbx_hint) = metadata
            .embedded_hint
            .as_any()
            .downcast_ref::<crate::texture_pipeline::FbxHint>()
        {
            return Ok(fbx_hint.texture_data.clone());
        }

        // Find the source that can handle this hint
        if let Some(source) = self.source_registry.find_source(&metadata.source_path) {
            return source.load_bytes(metadata.embedded_hint.as_ref());
        }

        anyhow::bail!(
            "No source found for file: {}",
            metadata.source_path.display()
        );
    }

    /// Parse loaded image data to macroquad format
    /// This uses the registered parsers to handle different image formats
    pub fn parse_image_data(&self, data: &LoadedImageData) -> Result<(Image, ImageInfo)> {
        for parser in &self.parsers {
            if parser.can_parse(data) {
                return parser.parse(data);
            }
        }

        anyhow::bail!("No parser found for image format: {:?}", data.format);
    }

    /// Convenience method: Convert EmbeddedMetadata to LoadedImageData
    /// This combines the load_bytes and metadata phases for easier usage
    pub fn metadata_to_loaded_data(&self, metadata: &EmbeddedMetadata) -> Result<LoadedImageData> {
        let data = self.load_bytes(metadata)?;

        Ok(LoadedImageData {
            name: metadata.name.clone(),
            data,
            file_size: metadata.file_size as usize,
            source_file: metadata.source_path.clone(),
            format: metadata.format,
            width: metadata.width,
            height: metadata.height,
        })
    }

    /// Process raw extracted data to detect containers/images recursively
    /// This is the core method for recursive container support
    pub fn extract_metadata_from_reader(
        &self,
        reader: &mut dyn BufReadSeek,
        entry_name: &str,
        parent_source_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>> {
        if let Some(source) = self.source_registry.find_source_for_reader(reader) {
            return source.extract_metadata_from_reader(reader, entry_name, parent_source_path);
        }

        // Return empty vec for unsupported formats instead of error
        log::debug!(
            "No source found for raw data entry: {} from {}",
            entry_name,
            parent_source_path.display()
        );
        Ok(Vec::new())
    }

    /// Find source that can handle raw data using can_load_reader()
    /// Used by container sources for delegation
    pub fn find_source_for_reader(&self, reader: &mut dyn BufReadSeek) -> Option<&dyn Source> {
        self.source_registry.find_source_for_reader(reader)
    }

    /// Get access to source registry (for debugging/testing)
    pub fn source_registry(&self) -> &SourceRegistry {
        &self.source_registry
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
