use anyhow::Result;
use std::io::{BufRead, Seek};
use std::path::Path;

use crate::texture_pipeline::{EmbeddedHint, EmbeddedMetadata};

/// Helper trait that combines BufRead + Seek for imagesize compatibility
pub trait BufReadSeek: BufRead + Seek {}

// Automatically implement BufReadSeek for any type that implements both BufRead and Seek
impl<T: BufRead + Seek> BufReadSeek for T {}

/// Unified trait for all texture sources (both containers and images)
/// Enhanced with recursive support according to refact_pipeline.md
pub trait Source: Send + Sync {
    /// Quick format detection from file path
    fn can_load_path(&self, path: &Path) -> Result<bool>;

    /// Quick format detection from reader (for embedded content & recursion)
    /// Uses BufReadSeek instead of BufRead for imagesize::reader_type compatibility
    fn can_load_reader(&self, reader: &mut dyn BufReadSeek) -> Result<bool>;

    /// Extract metadata from file path (containers and images)
    /// Parse container ONCE and create hints with direct access information
    fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>>;

    /// Extract metadata from raw data (enables recursive processing)
    /// This is the core method for recursive container support
    fn extract_metadata_from_reader(
        &self,
        reader: &mut dyn BufReadSeek,
        entry_name: &str,
        parent_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>>;

    /// Load raw bytes using hint (works for both embedded and direct files)
    /// Use hint's direct access information - no re-parsing needed
    fn load_bytes(&self, hint: &dyn EmbeddedHint) -> Result<Vec<u8>>;
}
