use std::any::{Any, TypeId};
use std::path::PathBuf;

/// Trait for embedded hints as specified in the refactoring plan
/// Extended with Any for downcasting capabilities
pub trait EmbeddedHint: Any + Send + Sync + 'static {
    fn debug_info(&self) -> String;

    /// Get the type ID for downcasting
    fn type_id(&self) -> TypeId
    where
        Self: Sized,
    {
        TypeId::of::<Self>()
    }

    /// Downcast to Any for type checking
    fn as_any(&self) -> &dyn Any;

    /// Optional header bytes for format detection (64 bytes max)
    /// Container sources can provide first 64 bytes of entries for fast format detection
    /// without requiring re-reading from the container
    fn header_bytes(&self) -> Option<&[u8]> {
        None // Default implementation - no header data
    }
}

/// Metadata for images (both direct files and embedded content)
pub struct EmbeddedMetadata {
    pub name: String,
    pub format: imagesize::ImageType,
    pub width: usize,
    pub height: usize,
    pub file_size: u64,
    pub embedded_hint: Box<dyn EmbeddedHint>,
    pub source_path: PathBuf,
}

impl Clone for EmbeddedMetadata {
    fn clone(&self) -> Self {
        // Create a new hint by downcasting and reconstructing
        let new_hint: Box<dyn EmbeddedHint> =
            if let Some(file_hint) = self.embedded_hint.as_any().downcast_ref::<FileHint>() {
                Box::new(file_hint.clone())
            } else if let Some(glb_hint) = self.embedded_hint.as_any().downcast_ref::<GlbHint>() {
                Box::new(glb_hint.clone())
            } else if let Some(fbx_hint) = self.embedded_hint.as_any().downcast_ref::<FbxHint>() {
                Box::new(fbx_hint.clone())
            } else if let Some(zip_hint) = self.embedded_hint.as_any().downcast_ref::<ZipHint>() {
                Box::new(zip_hint.clone())
            } else {
                panic!(
                    "Unknown hint type cannot be cloned: {}",
                    self.embedded_hint.debug_info()
                )
            };

        EmbeddedMetadata {
            name: self.name.clone(),
            format: self.format,
            width: self.width,
            height: self.height,
            file_size: self.file_size,
            embedded_hint: new_hint,
            source_path: self.source_path.clone(),
        }
    }
}

impl std::fmt::Debug for EmbeddedMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedMetadata")
            .field("name", &self.name)
            .field("format", &self.format)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("file_size", &self.file_size)
            .field("embedded_hint", &self.embedded_hint.debug_info())
            .field("source_path", &self.source_path)
            .finish()
    }
}

/// Hint for direct file loading
#[derive(Clone, Debug)]
pub struct FileHint {
    pub path: PathBuf,
}

impl EmbeddedHint for FileHint {
    fn debug_info(&self) -> String {
        format!("File[{}]", self.path.display())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Hint for GLB embedded textures
/// CRITICAL: This now contains ABSOLUTE file offset for direct access
/// For nested containers (ZIP→GLB), can store texture data directly
#[derive(Clone, Debug)]
pub struct GlbHint {
    pub container_path: PathBuf,
    pub buffer_index: usize,           // For cache lookup (if needed)
    pub absolute_file_offset: u64,     // NEW: Direct file offset
    pub length: usize,                 // Length in bytes
    pub relative_buffer_offset: usize, // OLD: Buffer-relative offset (for fallback)
    pub texture_data: Option<Vec<u8>>, // NEW: Direct texture data for nested containers
}

impl EmbeddedHint for GlbHint {
    fn debug_info(&self) -> String {
        let data_info = if self.texture_data.is_some() {
            "+data"
        } else {
            ""
        };
        format!(
            "GLB[buf:{}]@file:{}+{}{}",
            self.buffer_index, self.absolute_file_offset, self.length, data_info
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Hint for FBX embedded textures
/// According to refact_pipeline.md: Should contain direct access data
#[derive(Clone, Debug)]
pub struct FbxHint {
    pub container_path: PathBuf,
    pub texture_name: String,
    pub texture_index: usize,
    pub texture_data: Vec<u8>, // Direct data - no re-parsing needed!
}

impl EmbeddedHint for FbxHint {
    fn debug_info(&self) -> String {
        format!("FBX[{}]:{}", self.texture_index, self.texture_name)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Hint for ZIP embedded entries
/// Contains the entry name and index for direct access
/// Now includes optional header bytes for fast format detection
#[derive(Clone, Debug)]
pub struct ZipHint {
    pub container_path: PathBuf,
    pub entry_name: String,
    pub entry_index: usize,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub header_bytes: Option<Vec<u8>>, // First 64 bytes for format detection
}

impl EmbeddedHint for ZipHint {
    fn debug_info(&self) -> String {
        let header_info = if self.header_bytes.is_some() {
            "+header"
        } else {
            ""
        };
        format!(
            "ZIP[{}]:{}{}",
            self.entry_index, self.entry_name, header_info
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn header_bytes(&self) -> Option<&[u8]> {
        self.header_bytes.as_deref()
    }
}
