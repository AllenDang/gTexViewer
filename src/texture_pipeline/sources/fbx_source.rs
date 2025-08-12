use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;

use super::ultra_fast_fbx_parser::{TextureData, UltraFastFbxParser};
use crate::texture_pipeline::{BufReadSeek, EmbeddedHint, EmbeddedMetadata, FbxHint, Source};

pub struct FbxSource;

impl Source for FbxSource {
    fn can_load_path(&self, path: &Path) -> Result<bool> {
        // First check extension (fast)
        let has_fbx_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase() == "fbx")
            .unwrap_or(false);

        if !has_fbx_extension {
            return Ok(false);
        }

        // For FBX files, we could check magic bytes, but the ultra-fast parser
        // already handles invalid files gracefully, so we'll trust the extension
        Ok(true)
    }

    fn can_load_reader(&self, _reader: &mut dyn BufReadSeek) -> Result<bool> {
        // FBX files have complex binary format, checking magic bytes is not trivial
        // The ultra-fast parser can handle this check during actual parsing
        // For now, we'll return false since we primarily work with file paths
        Ok(false)
    }

    fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>> {
        // Use ultra-fast parser for texture extraction
        let mut parser = UltraFastFbxParser::new(path)?;
        let textures = parser.extract_textures()?;

        // Use rayon to parallelize texture processing
        let results: Result<Vec<_>, _> = textures
            .into_par_iter()
            .enumerate()
            .filter_map(|(index, texture_data)| {
                // Only process textures that have embedded content
                if texture_data.content.is_some() {
                    Some(self.convert_texture_to_metadata(texture_data, index, path))
                } else {
                    None
                }
            })
            .collect();

        let mut final_results = results?;

        // Make texture names unique if there are duplicates
        self.ensure_unique_names(&mut final_results);

        if final_results.is_empty() {
            log::warn!("No textures found in FBX file: {path:?}");
        } else {
            log::info!("FBX source extracted {} textures", final_results.len());
        }

        Ok(final_results)
    }

    fn load_bytes(&self, hint: &dyn EmbeddedHint) -> Result<Vec<u8>> {
        // Try to downcast to FbxHint
        if let Some(fbx_hint) = hint.as_any().downcast_ref::<FbxHint>() {
            // Use direct texture data - NO RE-PARSING!
            log::debug!(
                "Direct FBX access: returning {} bytes for texture '{}' in {}",
                fbx_hint.texture_data.len(),
                fbx_hint.texture_name,
                fbx_hint.container_path.display()
            );
            return Ok(fbx_hint.texture_data.clone());
        }

        anyhow::bail!("Invalid hint type for FBX source: {}", hint.debug_info())
    }

    fn extract_metadata_from_reader(
        &self,
        _reader: &mut dyn BufReadSeek,
        entry_name: &str,
        _parent_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>> {
        // FBX processing from reader not yet implemented
        log::debug!("FBX processing from reader not yet implemented for entry: {entry_name}");
        Ok(Vec::new())
    }
}

impl FbxSource {
    /// Convert TextureData from ultra-fast parser to EmbeddedMetadata
    fn convert_texture_to_metadata(
        &self,
        texture_data: TextureData,
        texture_index: usize,
        base_path: &Path,
    ) -> Result<EmbeddedMetadata> {
        let content = texture_data
            .content
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("FBX texture has no content"))?;

        // Use imagesize to detect format and dimensions from header
        let format = imagesize::image_type(content)?;
        let dimension = imagesize::blob_size(content)?;

        // Skip textures with invalid dimensions
        if dimension.width == 0 || dimension.height == 0 {
            anyhow::bail!(
                "Invalid dimensions for FBX texture {}: {}x{}",
                texture_data.name,
                dimension.width,
                dimension.height
            );
        }

        let hint = Box::new(FbxHint {
            container_path: base_path.to_path_buf(),
            texture_name: texture_data.name.clone(),
            texture_index,
            texture_data: content.clone(), // Store actual texture data for direct access
        }) as Box<dyn EmbeddedHint>;

        Ok(EmbeddedMetadata {
            name: texture_data.name,
            format,
            width: dimension.width,
            height: dimension.height,
            file_size: content.len() as u64,
            embedded_hint: hint,
            source_path: base_path.to_path_buf(),
        })
    }

    /// Ensure texture names are unique by appending indices if needed
    fn ensure_unique_names(&self, results: &mut [EmbeddedMetadata]) {
        let mut name_counters = std::collections::HashMap::new();

        for metadata in results.iter_mut() {
            if let Some(count) = name_counters.get_mut(&metadata.name) {
                *count += 1;
                metadata.name = format!("{}_{}", metadata.name, count);
            } else {
                name_counters.insert(metadata.name.clone(), 0);
            }
        }
    }
}
