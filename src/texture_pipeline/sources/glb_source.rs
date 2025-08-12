use anyhow::{Context, Result};
use gltf::{Gltf, buffer::Data, texture::Info as TextureInfo};
use std::collections::HashSet;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::texture_pipeline::{
    BufReadSeek, EmbeddedHint, EmbeddedMetadata, FileHint, GlbHint, Source,
};

pub struct GlbSource;

impl Source for GlbSource {
    fn can_load_path(&self, path: &Path) -> Result<bool> {
        // First check extension (fast)
        let has_glb_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_lowercase().as_str(), "glb" | "gltf"))
            .unwrap_or(false);

        if !has_glb_extension {
            return Ok(false);
        }

        // For GLB files, check magic bytes
        if path.extension().and_then(|ext| ext.to_str()) == Some("glb") {
            let mut file = std::fs::File::open(path)?;
            let mut header = [0u8; 4];
            file.read_exact(&mut header)?;
            Ok(&header == b"glTF")
        } else {
            // For GLTF files, assume valid if extension matches (they're JSON)
            Ok(true)
        }
    }

    fn can_load_reader(&self, reader: &mut dyn BufReadSeek) -> Result<bool> {
        let mut header = [0u8; 4];
        reader.read_exact(&mut header)?;
        Ok(&header == b"glTF")
    }

    fn extract_metadata(&self, path: &Path) -> Result<Vec<EmbeddedMetadata>> {
        // Load the GLB/GLTF file without validation to support KTX2 extensions
        // Parse container ONCE and create hints with absolute file offsets for direct access
        let file = std::fs::File::open(path).context("Failed to open GLB/GLTF file")?;
        let reader = BufReader::new(file);
        let gltf = Gltf::from_reader_without_validation(reader)
            .context("Failed to parse GLB/GLTF file")?;

        // Import buffers and calculate absolute file offsets
        let buffers_result = gltf::import_buffers(
            &gltf.document,
            Some(path.parent().unwrap_or(Path::new("."))),
            gltf.blob.clone(),
        )
        .context("Failed to import GLB buffers")?;

        // Calculate absolute file offsets for GLB blob data (if it's a GLB file)
        let glb_blob_offset = if path.extension().and_then(|e| e.to_str()) == Some("glb") {
            // GLB file structure: 12-byte header + JSON chunk + BIN chunk
            // JSON chunk: 8-byte chunk header + JSON data (padded to 4-byte boundary)
            // BIN chunk starts after JSON chunk

            // Re-read the GLB header to calculate offsets
            let mut file = std::fs::File::open(path)?;
            let mut header = [0u8; 12];
            file.read_exact(&mut header)?;

            // Read JSON chunk header
            let mut json_chunk_header = [0u8; 8];
            file.read_exact(&mut json_chunk_header)?;
            let json_length = u32::from_le_bytes([
                json_chunk_header[0],
                json_chunk_header[1],
                json_chunk_header[2],
                json_chunk_header[3],
            ]) as usize;

            // JSON chunk offset + padded JSON length + BIN chunk header = start of BIN data
            12 + 8 + ((json_length + 3) & !3) + 8
        } else {
            0 // GLTF files don't have embedded binary data in same file
        };

        let buffers = buffers_result;

        // Track processed texture indices to avoid duplicates
        let mut processed_texture_indices: HashSet<usize> = HashSet::new();

        // Sequentially process materials to track texture indices (no parallel processing here)
        let mut material_textures = Vec::new();

        for material in gltf.document.materials() {
            let material_name = material
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("material_{:?}", material.index()));

            // Process different texture types and track indices
            if let Some(texture_info) = material.pbr_metallic_roughness().base_color_texture() {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata(
                        &texture_info,
                        &format!("{material_name} - Base Color"),
                        &buffers,
                        path,
                        glb_blob_offset,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(texture_info) = material
                .pbr_metallic_roughness()
                .metallic_roughness_texture()
            {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata(
                        &texture_info,
                        &format!("{material_name} - Metallic Roughness"),
                        &buffers,
                        path,
                        glb_blob_offset,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(normal_tex) = material.normal_texture() {
                let texture_index = normal_tex.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_texture(
                        &normal_tex.texture(),
                        &format!("{material_name} - Normal"),
                        &buffers,
                        path,
                        glb_blob_offset,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(occlusion_tex) = material.occlusion_texture() {
                let texture_index = occlusion_tex.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_texture(
                        &occlusion_tex.texture(),
                        &format!("{material_name} - Occlusion"),
                        &buffers,
                        path,
                        glb_blob_offset,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(texture_info) = material.emissive_texture() {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata(
                        &texture_info,
                        &format!("{material_name} - Emissive"),
                        &buffers,
                        path,
                        glb_blob_offset,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }
        }

        // Also extract any standalone textures not referenced by materials
        let mut standalone_textures = Vec::new();
        for texture in gltf.document.textures() {
            let texture_index = texture.index();

            // Check if already processed by materials using texture index
            if !processed_texture_indices.contains(&texture_index) {
                let texture_name = texture
                    .name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("texture_{}", texture.index()));

                if let Ok(metadata) = self.extract_texture_metadata_from_texture(
                    &texture,
                    &format!("Standalone - {texture_name}"),
                    &buffers,
                    path,
                    glb_blob_offset,
                ) {
                    standalone_textures.push(metadata);
                }
            }
        }

        let mut results = material_textures;
        results.extend(standalone_textures);

        if results.is_empty() {
            anyhow::bail!("No valid textures found in GLB/GLTF file");
        }

        Ok(results)
    }

    fn load_bytes(&self, hint: &dyn EmbeddedHint) -> Result<Vec<u8>> {
        // Try to downcast to GlbHint first
        if let Some(glb_hint) = hint.as_any().downcast_ref::<GlbHint>() {
            // Check if we have direct texture data (for nested containers)
            if let Some(ref texture_data) = glb_hint.texture_data {
                return Ok(texture_data.clone());
            }

            // Otherwise use direct file access with absolute offset - NO RE-PARSING!
            return self.read_direct_file_slice(
                &glb_hint.container_path,
                glb_hint.absolute_file_offset,
                glb_hint.length,
            );
        }

        // Try to downcast to FileHint for external textures
        if let Some(file_hint) = hint.as_any().downcast_ref::<FileHint>() {
            return std::fs::read(&file_hint.path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read GLB external file {}: {}",
                    file_hint.path.display(),
                    e
                )
            });
        }

        anyhow::bail!("Invalid hint type for GLB source: {}", hint.debug_info())
    }

    fn extract_metadata_from_reader(
        &self,
        reader: &mut dyn BufReadSeek,
        entry_name: &str,
        parent_path: &Path,
    ) -> Result<Vec<EmbeddedMetadata>> {
        // Load the GLB/GLTF from reader without validation
        let gltf = Gltf::from_reader_without_validation(reader)
            .context("Failed to parse GLB/GLTF from reader")?;

        // For reader-based GLB processing, we assume it's GLB format (has blob)
        // Import buffers with blob data
        let buffers_result = if let Some(blob) = gltf.blob {
            vec![gltf::buffer::Data(blob)]
        } else {
            anyhow::bail!("GLB data from reader has no blob data");
        };

        let buffers = buffers_result;

        // Track processed texture indices to avoid duplicates
        let mut processed_texture_indices: HashSet<usize> = HashSet::new();
        let mut material_textures = Vec::new();

        // Sequentially process materials to track texture indices
        for material in gltf.document.materials() {
            let material_name = material
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("material_{:?}", material.index()));

            // Process all texture types with deduplication
            if let Some(texture_info) = material.pbr_metallic_roughness().base_color_texture() {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_reader(
                        &texture_info.texture(),
                        &format!("{material_name} - Base Color"),
                        &buffers,
                        parent_path,
                        entry_name,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(texture_info) = material
                .pbr_metallic_roughness()
                .metallic_roughness_texture()
            {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_reader(
                        &texture_info.texture(),
                        &format!("{material_name} - Metallic Roughness"),
                        &buffers,
                        parent_path,
                        entry_name,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(normal_tex) = material.normal_texture() {
                let texture_index = normal_tex.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_reader(
                        &normal_tex.texture(),
                        &format!("{material_name} - Normal"),
                        &buffers,
                        parent_path,
                        entry_name,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(occlusion_tex) = material.occlusion_texture() {
                let texture_index = occlusion_tex.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_reader(
                        &occlusion_tex.texture(),
                        &format!("{material_name} - Occlusion"),
                        &buffers,
                        parent_path,
                        entry_name,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }

            if let Some(texture_info) = material.emissive_texture() {
                let texture_index = texture_info.texture().index();
                if !processed_texture_indices.contains(&texture_index)
                    && let Ok(metadata) = self.extract_texture_metadata_from_reader(
                        &texture_info.texture(),
                        &format!("{material_name} - Emissive"),
                        &buffers,
                        parent_path,
                        entry_name,
                    )
                {
                    material_textures.push(metadata);
                    processed_texture_indices.insert(texture_index);
                }
            }
        }

        if material_textures.is_empty() {
            anyhow::bail!("No valid textures found in GLB data from reader");
        }

        Ok(material_textures)
    }
}

impl GlbSource {
    fn extract_texture_metadata(
        &self,
        texture_info: &TextureInfo,
        texture_type: &str,
        buffers: &[Data],
        base_path: &Path,
        glb_blob_offset: usize,
    ) -> Result<EmbeddedMetadata> {
        self.extract_texture_metadata_from_texture(
            &texture_info.texture(),
            texture_type,
            buffers,
            base_path,
            glb_blob_offset,
        )
    }

    fn extract_texture_metadata_from_texture(
        &self,
        texture: &gltf::Texture,
        texture_type: &str,
        buffers: &[Data],
        base_path: &Path,
        glb_blob_offset: usize,
    ) -> Result<EmbeddedMetadata> {
        let image = texture.source();
        let source = image.source();

        match source {
            gltf::image::Source::View { view, mime_type: _ } => {
                // Get image data size from buffer view (without actually reading the full data)
                let file_size = view.length() as u64;

                // For embedded textures, we need to read a small header for format detection
                let buffer_data = &buffers[view.buffer().index()];
                let start = view.offset();
                let end = start + std::cmp::min(view.length(), 1024); // Read max 1KB for format detection
                let header_data = &buffer_data.0[start..end];

                // Detect format from the header data
                let format = imagesize::image_type(header_data)?;

                // Try to get dimensions from the header data
                let dimension = imagesize::blob_size(header_data)?;

                // Skip textures with invalid dimensions
                if dimension.width == 0 || dimension.height == 0 {
                    anyhow::bail!(
                        "Invalid dimensions for GLB texture {}: {}x{}",
                        texture_type,
                        dimension.width,
                        dimension.height
                    );
                }

                let hint = Box::new(GlbHint {
                    container_path: base_path.to_path_buf(),
                    buffer_index: view.buffer().index(),
                    absolute_file_offset: (glb_blob_offset + view.offset()) as u64,
                    length: view.length(),
                    relative_buffer_offset: view.offset(),
                    texture_data: None, // No direct data for file-based access
                }) as Box<dyn EmbeddedHint>;

                Ok(EmbeddedMetadata {
                    name: texture_type.to_string(),
                    format,
                    width: dimension.width,
                    height: dimension.height,
                    file_size,
                    embedded_hint: hint,
                    source_path: base_path.to_path_buf(),
                })
            }
            gltf::image::Source::Uri { uri, mime_type: _ } => {
                // Handle external image files referenced by URI
                let image_path = if Path::new(uri).is_absolute() {
                    Path::new(uri).to_path_buf()
                } else {
                    base_path.parent().unwrap_or(Path::new(".")).join(uri)
                };

                // For external files, read just the header for metadata
                let file = std::fs::File::open(&image_path)?;
                let mut reader = BufReader::new(file);

                let format = imagesize::reader_type(&mut reader)?;
                reader.seek(SeekFrom::Start(0))?;
                let dimension = imagesize::reader_size(&mut reader)?;
                let file_size = std::fs::metadata(&image_path)?.len();

                // Skip textures with invalid dimensions
                if dimension.width == 0 || dimension.height == 0 {
                    anyhow::bail!(
                        "Invalid dimensions for GLB external texture {}: {}x{}",
                        texture_type,
                        dimension.width,
                        dimension.height
                    );
                }

                // For external files, use FileHint
                let hint = Box::new(FileHint {
                    path: image_path.clone(),
                }) as Box<dyn EmbeddedHint>;

                Ok(EmbeddedMetadata {
                    name: texture_type.to_string(),
                    format,
                    width: dimension.width,
                    height: dimension.height,
                    file_size,
                    embedded_hint: hint,
                    source_path: image_path,
                })
            }
        }
    }

    /// Direct file access using absolute file offset - NO RE-PARSING!
    /// This is the key to the hint system working properly
    fn read_direct_file_slice(
        &self,
        glb_path: &Path,
        absolute_offset: u64,
        length: usize,
    ) -> Result<Vec<u8>> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let mut file = File::open(glb_path).context("Failed to open GLB file for direct access")?;

        // Seek to the absolute offset
        file.seek(SeekFrom::Start(absolute_offset))
            .context("Failed to seek to texture data offset")?;

        // Read the exact number of bytes
        let mut buffer = vec![0u8; length];
        file.read_exact(&mut buffer)
            .context("Failed to read texture data from GLB file")?;

        log::debug!(
            "Direct GLB access: read {} bytes from offset {} in {}",
            length,
            absolute_offset,
            glb_path.display()
        );

        Ok(buffer)
    }

    /// Extract texture metadata from reader-based GLB processing
    fn extract_texture_metadata_from_reader(
        &self,
        texture: &gltf::Texture,
        texture_type: &str,
        buffers: &[gltf::buffer::Data],
        parent_path: &Path,
        container_name: &str,
    ) -> Result<EmbeddedMetadata> {
        let image = texture.source();
        let source = image.source();

        match source {
            gltf::image::Source::View { view, mime_type: _ } => {
                // Get image data size from buffer view
                let file_size = view.length() as u64;

                // For embedded textures, read header for format detection
                let buffer_data = &buffers[view.buffer().index()];
                let start = view.offset();
                let end = start + std::cmp::min(view.length(), 1024);
                let header_data = &buffer_data.0[start..end];

                // Detect format and dimensions
                let format = imagesize::image_type(header_data)?;
                let dimension = imagesize::blob_size(header_data)?;

                // Skip textures with invalid dimensions
                if dimension.width == 0 || dimension.height == 0 {
                    anyhow::bail!(
                        "Invalid dimensions for GLB texture {}: {}x{}",
                        texture_type,
                        dimension.width,
                        dimension.height
                    );
                }

                // Extract the actual texture data and store it in GlbHint
                // This avoids the hint mismatch issue with nested containers
                let buffer_data = &buffers[view.buffer().index()];
                let texture_data =
                    buffer_data.0[view.offset()..view.offset() + view.length()].to_vec();

                let hint = Box::new(GlbHint {
                    container_path: parent_path.to_path_buf(),
                    buffer_index: view.buffer().index(),
                    absolute_file_offset: 0, // Not applicable for nested container
                    length: view.length(),
                    relative_buffer_offset: view.offset(),
                    texture_data: Some(texture_data), // Store actual data for nested container
                }) as Box<dyn EmbeddedHint>;

                Ok(EmbeddedMetadata {
                    name: format!("{container_name} - {texture_type}"),
                    format,
                    width: dimension.width,
                    height: dimension.height,
                    file_size,
                    embedded_hint: hint,
                    source_path: parent_path.to_path_buf(), // Keep original path for reference
                })
            }
            gltf::image::Source::Uri {
                uri: _,
                mime_type: _,
            } => {
                anyhow::bail!("External URI textures not supported in reader-based GLB processing")
            }
        }
    }
}
