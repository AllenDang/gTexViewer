use rayon::prelude::*;
use std::path::{Path, PathBuf};

use crate::texture_pipeline::{EmbeddedMetadata, Source};

/// Registry that holds all available texture sources
pub struct SourceRegistry {
    sources: Vec<Box<dyn Source>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Add a source to the registry
    pub fn add_source(&mut self, source: Box<dyn Source>) {
        self.sources.push(source);
    }

    /// Find the first source that can handle the given path
    pub fn find_source(&self, path: &Path) -> Option<&dyn Source> {
        self.sources
            .iter()
            .find(|source| source.can_load_path(path).unwrap_or(false))
            .map(|s| s.as_ref())
    }

    /// Find source for raw data (enables recursive processing)
    /// This is core to the recursive container architecture
    pub fn find_source_for_reader(
        &self,
        reader: &mut dyn crate::texture_pipeline::BufReadSeek,
    ) -> Option<&dyn Source> {
        self.sources
            .iter()
            .find(|source| source.can_load_reader(reader).unwrap_or(false))
            .map(|s| s.as_ref())
    }

    /// Parallel metadata extraction for multiple files
    /// Uses rayon to process files concurrently for better performance
    /// According to refact_pipeline.md - each source parses container once and creates proper hints
    pub fn extract_all_metadata_parallel(&self, paths: Vec<PathBuf>) -> Vec<EmbeddedMetadata> {
        paths
            .into_par_iter()
            .filter_map(|path| {
                // Find source in parallel - each thread searches independently
                let source = self
                    .sources
                    .par_iter()
                    .find_any(|s| s.can_load_path(&path).unwrap_or(false))?;

                // Extract metadata using the found source
                match source.extract_metadata(&path) {
                    Ok(metadata_list) => Some(metadata_list),
                    Err(e) => {
                        log::warn!("Failed to extract metadata from {}: {}", path.display(), e);
                        None
                    }
                }
            })
            .flatten() // Flatten Vec<Vec<EmbeddedMetadata>> to Vec<EmbeddedMetadata>
            .collect()
    }

    /// Get all registered sources (for debugging/testing)
    pub fn sources(&self) -> &[Box<dyn Source>] {
        &self.sources
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
