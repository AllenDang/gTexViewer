use anyhow::{Context, Result};
use std::io::{BufReader, Read, SeekFrom};
use std::path::Path;
use zip::ZipArchive;

use crate::texture_pipeline::{BufReadSeek, EmbeddedHint, EmbeddedMetadata, Source, ZipHint};

pub struct ZipSource;

impl Source for ZipSource {
    fn can_load_path(&self, path: &Path) -> Result<bool> {
        // First check extension (fast)
        let has_zip_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_lowercase().as_str(), "zip"))
            .unwrap_or(false);

        if !has_zip_extension {
            return Ok(false);
        }

        // Try to open as ZIP archive to verify format
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        match ZipArchive::new(reader) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn can_load_reader(&self, reader: &mut dyn BufReadSeek) -> Result<bool> {
        // Check ZIP magic bytes (PK\x03\x04 or PK\x05\x06 for empty archives)
        let mut header = [0u8; 4];
        reader.read_exact(&mut header)?;

        // Reset reader position
        reader.seek(SeekFrom::Start(0))?;

        // ZIP local file header magic or central directory end magic
        Ok(&header == b"PK\x03\x04" || &header == b"PK\x05\x06")
    }

    fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>> {
        let file = std::fs::File::open(path).context("Failed to open ZIP file")?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader).context("Failed to read ZIP archive")?;

        let mut metadata_list = Vec::new();

        // Process entries with header extraction for fast format detection
        for i in 0..archive.len() {
            let entry_result = (|| -> Result<Option<EmbeddedMetadata>> {
                let mut entry = archive.by_index(i)?;

                // Skip directories early
                if entry.is_dir() {
                    return Ok(None);
                }

                let entry_name = entry.name().to_string();
                let compressed_size = entry.compressed_size();
                let uncompressed_size = entry.size();

                // Extract header bytes incrementally for format detection
                let header_bytes = if uncompressed_size > 0 {
                    Self::read_header_incrementally(&mut entry, uncompressed_size as usize)?
                } else {
                    None
                };

                // Create hint for this ZIP entry with header bytes
                let hint = Box::new(ZipHint {
                    container_path: path.to_path_buf(),
                    entry_name: entry_name.clone(),
                    entry_index: i,
                    compressed_size,
                    uncompressed_size,
                    header_bytes: header_bytes.clone(),
                }) as Box<dyn EmbeddedHint>;

                // Skip entries with no content
                if uncompressed_size == 0 {
                    return Ok(None);
                }

                // Pure container extraction - ZipSource doesn't do format detection
                // The header bytes are provided for Pipeline to use for format detection
                // Pipeline will determine actual format, dimensions, and handle recursive processing

                let metadata = EmbeddedMetadata {
                    name: entry_name,
                    format: imagesize::ImageType::Png, // Placeholder - Pipeline will determine actual format
                    width: 0,                          // Pipeline will determine actual dimensions
                    height: 0,                         // Pipeline will determine actual dimensions
                    file_size: uncompressed_size,
                    embedded_hint: hint,
                    source_path: path.to_path_buf(),
                };

                Ok(Some(metadata))
            })();

            match entry_result {
                Ok(Some(metadata)) => metadata_list.push(metadata),
                Ok(None) => {} // Skip directories and non-images
                Err(e) => {
                    log::debug!("Failed to extract metadata from ZIP entry {i}: {e}");
                    // Continue processing other entries even if one fails
                }
            }
        }

        if metadata_list.is_empty() {
            anyhow::bail!("No entries found in ZIP archive");
        }

        log::info!(
            "ZIP container extraction completed: {} entries from {}",
            metadata_list.len(),
            path.display()
        );

        Ok(metadata_list)
    }

    fn load_bytes(&self, hint: &dyn EmbeddedHint) -> Result<Vec<u8>> {
        // Try to downcast to ZipHint
        if let Some(zip_hint) = hint.as_any().downcast_ref::<ZipHint>() {
            return self.read_zip_entry(zip_hint);
        }

        anyhow::bail!("Invalid hint type for ZIP source: {}", hint.debug_info())
    }

    fn extract_metadata_from_reader(
        &self,
        _reader: &mut dyn BufReadSeek,
        entry_name: &str,
        _parent_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>> {
        // ZIP processing from reader (ZIP-in-ZIP scenarios) not yet implemented
        log::debug!("ZIP processing from reader not yet implemented for entry: {entry_name}");
        Ok(Vec::new())
    }
}

impl ZipSource {
    /// Read header bytes incrementally until imagesize can determine dimensions
    /// or we reach a reasonable maximum size
    fn read_header_incrementally<R: Read>(
        entry: &mut zip::read::ZipFile<R>,
        max_size: usize,
    ) -> Result<Option<Vec<u8>>> {
        let mut header_size = 128; // Start small - most formats store dimensions early
        let max_header_size = std::cmp::min(65536, max_size); // Cap at 64KB or file size
        let mut accumulated_buffer = Vec::new();

        while header_size <= max_header_size {
            // Calculate how much more we need to read
            let bytes_to_read = header_size.saturating_sub(accumulated_buffer.len());
            if bytes_to_read == 0 {
                break;
            }

            // Read additional bytes
            let mut temp_buffer = vec![0u8; bytes_to_read];
            let bytes_read = entry.read(&mut temp_buffer)?;

            if bytes_read == 0 {
                // No more data available
                break;
            }

            temp_buffer.truncate(bytes_read);
            accumulated_buffer.extend(temp_buffer);

            // Try to determine image dimensions with current buffer
            if let Ok(_dimensions) = imagesize::blob_size(&accumulated_buffer) {
                log::debug!(
                    "Header size determined with {} bytes (started at {}, max {})",
                    accumulated_buffer.len(),
                    128,
                    max_header_size
                );
                return Ok(Some(accumulated_buffer));
            }

            // If we've read all available data, stop trying
            if accumulated_buffer.len() >= max_size {
                break;
            }

            // Increase buffer size for next iteration
            header_size = std::cmp::min(header_size + 1024, max_header_size);
        }

        log::debug!(
            "Header reading completed with {} bytes (imagesize couldn't determine dimensions)",
            accumulated_buffer.len()
        );

        // Return whatever we have, even if imagesize couldn't determine dimensions
        // The pipeline might still be able to process it
        Ok(Some(accumulated_buffer))
    }

    /// Read a specific entry from the ZIP archive using the hint information
    fn read_zip_entry(&self, hint: &ZipHint) -> Result<Vec<u8>> {
        let file = std::fs::File::open(&hint.container_path)
            .context("Failed to open ZIP file for reading entry")?;
        let reader = BufReader::new(file);
        let mut archive =
            ZipArchive::new(reader).context("Failed to read ZIP archive for entry")?;

        let mut entry = archive
            .by_index(hint.entry_index)
            .with_context(|| format!("Failed to find ZIP entry at index {}", hint.entry_index))?;

        // Verify entry name matches (safety check)
        if entry.name() != hint.entry_name {
            anyhow::bail!(
                "ZIP entry name mismatch: expected '{}', found '{}'",
                hint.entry_name,
                entry.name()
            );
        }

        // Read the entire entry
        let mut buffer = Vec::with_capacity(hint.uncompressed_size as usize);
        entry
            .read_to_end(&mut buffer)
            .with_context(|| format!("Failed to read ZIP entry: {}", hint.entry_name))?;

        log::debug!(
            "ZIP entry read: {} bytes from entry '{}' in {}",
            buffer.len(),
            hint.entry_name,
            hint.container_path.display()
        );

        Ok(buffer)
    }
}
