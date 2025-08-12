use anyhow::Result;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use crate::texture_pipeline::{BufReadSeek, EmbeddedHint, EmbeddedMetadata, FileHint, Source};

/// Universal image source that handles all standard image formats via imagesize
pub struct ImageSource;

impl Source for ImageSource {
    fn can_load_path(&self, path: &Path) -> Result<bool> {
        // Use imagesize to detect if this is a valid image file
        let file = std::fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        Ok(imagesize::reader_type(&mut reader).is_ok())
    }

    fn can_load_reader(&self, reader: &mut dyn BufReadSeek) -> Result<bool> {
        // Use imagesize::reader_type which handles the header reading internally
        Ok(imagesize::reader_type(reader).is_ok())
    }

    fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>> {
        // Open file and create buffered reader
        let file = std::fs::File::open(path)?;
        let mut reader = BufReader::new(file);

        // Use imagesize for format detection
        let format = imagesize::reader_type(&mut reader)?;

        // Reset reader position for dimensions
        reader.seek(SeekFrom::Start(0))?;
        let dimension = imagesize::reader_size(&mut reader)?;

        // Get file size
        let file_size = std::fs::metadata(path)?.len();

        // Skip files with invalid dimensions
        if dimension.width == 0 || dimension.height == 0 {
            anyhow::bail!(
                "Invalid dimensions for image file {}: {}x{}",
                path.display(),
                dimension.width,
                dimension.height
            );
        }

        // Create file hint for direct file loading
        let hint = Box::new(FileHint {
            path: path.to_path_buf(),
        }) as Box<dyn EmbeddedHint>;

        let metadata = EmbeddedMetadata {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string(),
            format,
            width: dimension.width,
            height: dimension.height,
            file_size,
            embedded_hint: hint,
            source_path: path.to_path_buf(),
        };

        Ok(vec![metadata])
    }

    fn extract_metadata_from_reader(
        &self,
        reader: &mut dyn BufReadSeek,
        entry_name: &str,
        parent_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>> {
        // Use imagesize for format detection from reader
        let format = imagesize::reader_type(&mut *reader)?;

        // Reset reader position for dimensions
        reader.seek(SeekFrom::Start(0))?;
        let dimension = imagesize::reader_size(&mut *reader)?;

        // Skip files with invalid dimensions
        if dimension.width == 0 || dimension.height == 0 {
            anyhow::bail!(
                "Invalid dimensions for image entry {}: {}x{}",
                entry_name,
                dimension.width,
                dimension.height
            );
        }

        // For reader-based processing, we need to read the data and create a FileHint
        // pointing to the parent path (this will be used by the container to load the data)
        // Note: This is a simplification - in a full implementation, we'd need nested hints
        let hint = Box::new(FileHint {
            path: parent_path.to_path_buf(),
        }) as Box<dyn EmbeddedHint>;

        let metadata = EmbeddedMetadata {
            name: entry_name.to_string(),
            format,
            width: dimension.width,
            height: dimension.height,
            file_size: 0, // Will be set by the container source
            embedded_hint: hint,
            source_path: parent_path.to_path_buf(),
        };

        Ok(vec![metadata])
    }

    fn load_bytes(&self, hint: &dyn EmbeddedHint) -> Result<Vec<u8>> {
        // Try to downcast to FileHint
        if let Some(file_hint) = hint.as_any().downcast_ref::<FileHint>() {
            return std::fs::read(&file_hint.path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read image file {}: {}",
                    file_hint.path.display(),
                    e
                )
            });
        }

        anyhow::bail!("Invalid hint type for Image source: {}", hint.debug_info())
    }
}
