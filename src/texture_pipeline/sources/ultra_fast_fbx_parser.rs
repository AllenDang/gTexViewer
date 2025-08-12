use anyhow::Result;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub struct UltraFastFbxParser {
    reader: BufReader<std::fs::File>,
    file_size: u64,
    fbx_version: u32,
}

#[derive(Debug)]
struct FbxNode {
    name: String,
    properties: Vec<Vec<u8>>,
    end_offset: u64,
}

#[derive(Debug)]
pub struct TextureData {
    pub name: String,
    pub relative_filename: Option<String>,
    pub content: Option<Vec<u8>>,
}

impl UltraFastFbxParser {
    /// Create new ultra-fast FBX parser
    pub fn new(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);

        Ok(Self {
            reader,
            file_size,
            fbx_version: 0,
        })
    }

    /// Ultra-fast texture extraction - proper FBX binary parsing
    pub fn extract_textures(&mut self) -> Result<Vec<TextureData>> {
        log::info!("🚀 Ultra-fast FBX texture extraction starting");
        let start_time = std::time::Instant::now();

        // Read and verify FBX header
        let mut magic = vec![0u8; 21];
        self.reader.read_exact(&mut magic)?;

        let magic_str = String::from_utf8_lossy(&magic);
        if !magic_str.starts_with("Kaydara FBX Binary") {
            return Err(anyhow::anyhow!("Invalid FBX file: magic header mismatch"));
        }

        // Read version info (2 bytes unknown + 4 bytes version)
        let mut version_data = [0u8; 6];
        self.reader.read_exact(&mut version_data)?;
        self.fbx_version = u32::from_le_bytes([
            version_data[2],
            version_data[3],
            version_data[4],
            version_data[5],
        ]);
        log::debug!("📋 FBX version: {}", self.fbx_version);

        let textures = self.parse_fbx_nodes_for_textures()?;

        let elapsed = start_time.elapsed();
        log::info!(
            "⚡ Found {} textures in {:.2}s",
            textures.len(),
            elapsed.as_secs_f64()
        );

        Ok(textures)
    }

    /// Parse FBX nodes looking specifically for texture data
    fn parse_fbx_nodes_for_textures(&mut self) -> Result<Vec<TextureData>> {
        let mut textures = Vec::new();
        let mut node_count = 0;

        log::info!("📦 Starting FBX node parsing...");

        loop {
            let current_pos = self.reader.stream_position()?;

            // Stop if we're near end of file
            if current_pos >= self.file_size - 50 {
                log::debug!("📍 Reached end of file at position {current_pos}");
                break;
            }

            match self.read_fbx_node() {
                Ok(Some(node)) => {
                    node_count += 1;
                    log::debug!("📦 Node #{}: '{}'", node_count, node.name);

                    if node.name == "Video" || node.name == "Texture" {
                        log::info!("🎯 Found texture node: {}", node.name);
                        if let Some(texture_data) = self.extract_texture_from_fbx_node(&node)? {
                            log::info!(
                                "✅ Extracted texture #{}: {}",
                                textures.len() + 1,
                                texture_data.name
                            );
                            textures.push(texture_data);
                        }
                    } else if node.name == "Objects" {
                        log::info!("🔍 Parsing Objects node for textures...");
                        self.parse_objects_node_for_textures(&node, &mut textures)?;
                    } else {
                        // Skip to end of this node
                        self.reader.seek(SeekFrom::Start(node.end_offset))?;
                    }
                }
                Ok(None) => {
                    log::debug!("📄 Reached null node (end)");
                    break;
                }
                Err(e) => {
                    log::debug!("⚠️ Error reading node at {current_pos}: {e}");
                    // Skip ahead and try to continue
                    if current_pos + 100 < self.file_size {
                        self.reader.seek(SeekFrom::Start(current_pos + 100))?;
                    } else {
                        break;
                    }
                }
            }

            // Safety limit
            if node_count > 10000 {
                log::warn!("⚠️ Safety limit reached - processed {node_count} nodes");
                break;
            }
        }

        log::info!(
            "📊 Processed {} nodes, found {} textures",
            node_count,
            textures.len()
        );
        Ok(textures)
    }

    /// Read a single FBX node from current position
    fn read_fbx_node(&mut self) -> Result<Option<FbxNode>> {
        let pos_before_header = self.reader.stream_position()?;

        // FBX node structure differs by version:
        // v7.5+: 25 bytes (8+8+8+1) - end_offset: u64, num_properties: u64, property_list_len: u64, name_len: u8
        // v7.4 and below: 13 bytes (4+4+4+1) - end_offset: u32, num_properties: u32, property_list_len: u32, name_len: u8

        let (end_offset, num_properties, property_list_len, name_len) = if self.fbx_version >= 7500
        {
            // 25-byte header for v7.5+
            let mut header = [0u8; 25];
            if self.reader.read_exact(&mut header).is_err() {
                return Ok(None); // End of file
            }

            let end_offset = u64::from_le_bytes([
                header[0], header[1], header[2], header[3], header[4], header[5], header[6],
                header[7],
            ]);

            let num_properties = u64::from_le_bytes([
                header[8], header[9], header[10], header[11], header[12], header[13], header[14],
                header[15],
            ]);

            let property_list_len = u64::from_le_bytes([
                header[16], header[17], header[18], header[19], header[20], header[21], header[22],
                header[23],
            ]);

            let name_len = header[24];

            (end_offset, num_properties, property_list_len, name_len)
        } else {
            // 13-byte header for v7.4 and below
            let mut header = [0u8; 13];
            if self.reader.read_exact(&mut header).is_err() {
                return Ok(None); // End of file
            }

            let end_offset =
                u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as u64;
            let num_properties =
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
            let property_list_len =
                u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as u64;
            let name_len = header[12];

            (end_offset, num_properties, property_list_len, name_len)
        };

        log::debug!(
            "🔍 Header parsed: end={end_offset}, props={num_properties}, len={property_list_len}, name_len={name_len}"
        );

        // Check for null node (end marker) - both end_offset and name_len must be 0
        if end_offset == 0 && name_len == 0 {
            log::debug!("🔍 Found null node at pos {pos_before_header}");
            return Ok(None);
        }

        // Sanity checks
        if name_len > 100 || property_list_len > (1 << 30) || end_offset > self.file_size * 2 {
            log::debug!(
                "🚨 Suspicious values: name_len={}, prop_len={}, end_offset={}, file_size={}",
                name_len,
                property_list_len,
                end_offset,
                self.file_size
            );
            return Err(anyhow::anyhow!(
                "Invalid node header values at pos {}",
                pos_before_header
            ));
        }

        // Read node name
        let mut name_bytes = vec![0u8; name_len as usize];
        self.reader.read_exact(&mut name_bytes)?;

        let name = String::from_utf8(name_bytes).map_err(|_| {
            anyhow::anyhow!("Invalid UTF-8 in node name at pos {}", pos_before_header)
        })?;

        log::debug!(
            "🔧 Node '{name}' at {pos_before_header}: end={end_offset}, props={num_properties}, len={property_list_len}"
        );

        // Read properties (we'll parse them later if needed)
        let mut properties_data = vec![0u8; property_list_len as usize];
        self.reader.read_exact(&mut properties_data)?;

        let node = FbxNode {
            name,
            properties: vec![properties_data], // Store raw property data
            end_offset,
        };

        Ok(Some(node))
    }

    /// Parse the Objects node looking for Video/Texture children
    fn parse_objects_node_for_textures(
        &mut self,
        objects_node: &FbxNode,
        textures: &mut Vec<TextureData>,
    ) -> Result<()> {
        let children_start = self.reader.stream_position()?;
        log::debug!(
            "🔍 Parsing Objects children from {} to {}",
            children_start,
            objects_node.end_offset
        );

        let mut child_count = 0;

        // Parse all child nodes until we reach the end of this Objects node
        while self.reader.stream_position()? < objects_node.end_offset {
            match self.read_fbx_node() {
                Ok(Some(child)) => {
                    child_count += 1;
                    log::debug!("🔍 Objects child #{}: '{}'", child_count, child.name);

                    if child.name == "Video" || child.name == "Texture" {
                        log::info!("🎯 Found texture node in Objects: {}", child.name);
                        if let Some(texture_data) = self.extract_texture_from_fbx_node(&child)? {
                            log::info!(
                                "✅ Extracted texture #{}: {}",
                                textures.len() + 1,
                                texture_data.name
                            );
                            textures.push(texture_data);
                        }
                    } else {
                        // Skip to end of this child node
                        self.reader.seek(SeekFrom::Start(child.end_offset))?;
                    }
                }
                Ok(None) => {
                    log::debug!("🔍 No more children in Objects node");
                    break;
                }
                Err(e) => {
                    log::debug!("⚠️ Error reading Objects child: {e}");
                    break;
                }
            }
        }

        log::debug!("🔍 Processed {child_count} children in Objects node");
        Ok(())
    }

    /// Extract texture data from Video/Texture node
    fn extract_texture_from_fbx_node(&mut self, node: &FbxNode) -> Result<Option<TextureData>> {
        log::debug!("🔍 Extracting texture from {} node", node.name);

        let mut texture_data = TextureData {
            name: node.name.clone(),
            relative_filename: None,
            content: None,
        };

        // Children start after the current position (we've already read name + properties)
        let children_start_pos = self.reader.stream_position()?;

        log::debug!(
            "🔍 Parsing children from {} to {} for {} node",
            children_start_pos,
            node.end_offset,
            node.name
        );

        // Parse all child nodes until we reach the end of this node
        while self.reader.stream_position()? < node.end_offset {
            match self.read_fbx_node() {
                Ok(Some(child)) => {
                    log::debug!("🔍 Child node: {}", child.name);

                    match child.name.as_str() {
                        "RelativeFilename" | "RelativeFileName" => {
                            if let Some(filename) =
                                self.extract_string_from_properties(&child.properties)
                            {
                                texture_data.relative_filename = Some(filename);
                                log::debug!(
                                    "📁 Found filename: {:?}",
                                    texture_data.relative_filename
                                );
                            }
                            // Skip to end of this child node
                            self.reader.seek(SeekFrom::Start(child.end_offset))?;
                        }
                        "Content" => {
                            if let Some(content) =
                                self.extract_binary_from_properties(&child.properties)
                                && !content.is_empty()
                            {
                                texture_data.content = Some(content);
                                log::debug!(
                                    "💾 Found content: {} bytes",
                                    texture_data.content.as_ref().unwrap().len()
                                );
                            }
                            // Skip to end of this child node
                            self.reader.seek(SeekFrom::Start(child.end_offset))?;
                        }
                        _ => {
                            log::debug!("⏭️ Skipping child: {}", child.name);
                            // Skip to end of this child node
                            self.reader.seek(SeekFrom::Start(child.end_offset))?;
                        }
                    }
                }
                Ok(None) => {
                    log::debug!("🔍 No more children for {} node", node.name);
                    break;
                }
                Err(e) => {
                    log::debug!("⚠️ Error reading child node: {e}");
                    break;
                }
            }
        }

        // Ensure we're at the end of this node
        self.reader.seek(SeekFrom::Start(node.end_offset))?;

        // Return texture data if we found content or filename
        if texture_data.content.is_some() || texture_data.relative_filename.is_some() {
            Ok(Some(texture_data))
        } else {
            log::debug!("⚠️ No content or filename found for {} node", node.name);
            Ok(None)
        }
    }

    /// Extract string from property data
    fn extract_string_from_properties(&self, properties: &[Vec<u8>]) -> Option<String> {
        if properties.is_empty() {
            return None;
        }

        let prop_data = &properties[0];
        let mut offset = 0;

        // Parse each property value in the property data
        while offset + 5 <= prop_data.len() {
            let value_type = prop_data[offset];

            match value_type {
                b'S' => {
                    // FBX string property: type 'S' + 4-byte length + string data
                    let len = u32::from_le_bytes([
                        prop_data[offset + 1],
                        prop_data[offset + 2],
                        prop_data[offset + 3],
                        prop_data[offset + 4],
                    ]) as usize;

                    if offset + 5 + len <= prop_data.len() {
                        let string_bytes = &prop_data[offset + 5..offset + 5 + len];
                        if let Ok(s) = String::from_utf8(string_bytes.to_vec()) {
                            return Some(s);
                        }
                    }
                    offset += 5 + len;
                }
                b'R' => {
                    // Skip binary data
                    let len = u32::from_le_bytes([
                        prop_data[offset + 1],
                        prop_data[offset + 2],
                        prop_data[offset + 3],
                        prop_data[offset + 4],
                    ]) as usize;
                    offset += 5 + len;
                }
                b'I' => offset += 5, // 4-byte int
                b'L' => offset += 9, // 8-byte long
                b'F' => offset += 5, // 4-byte float
                b'D' => offset += 9, // 8-byte double
                b'Y' => offset += 3, // 2-byte short
                b'C' => offset += 2, // 1-byte char/bool
                _ => {
                    log::debug!("Unknown property type: 0x{value_type:02X}");
                    break;
                }
            }
        }

        None
    }

    /// Extract binary data from property data  
    fn extract_binary_from_properties(&self, properties: &[Vec<u8>]) -> Option<Vec<u8>> {
        if properties.is_empty() {
            return None;
        }

        let prop_data = &properties[0];
        let mut offset = 0;

        // Parse each property value in the property data
        while offset + 5 <= prop_data.len() {
            let value_type = prop_data[offset];

            match value_type {
                b'R' => {
                    // FBX binary property: type 'R' + 4-byte length + binary data
                    let len = u32::from_le_bytes([
                        prop_data[offset + 1],
                        prop_data[offset + 2],
                        prop_data[offset + 3],
                        prop_data[offset + 4],
                    ]) as usize;

                    if offset + 5 + len <= prop_data.len() {
                        return Some(prop_data[offset + 5..offset + 5 + len].to_vec());
                    }
                    offset += 5 + len;
                }
                b'S' => {
                    // Skip string data
                    let len = u32::from_le_bytes([
                        prop_data[offset + 1],
                        prop_data[offset + 2],
                        prop_data[offset + 3],
                        prop_data[offset + 4],
                    ]) as usize;
                    offset += 5 + len;
                }
                b'I' => offset += 5, // 4-byte int
                b'L' => offset += 9, // 8-byte long
                b'F' => offset += 5, // 4-byte float
                b'D' => offset += 9, // 8-byte double
                b'Y' => offset += 3, // 2-byte short
                b'C' => offset += 2, // 1-byte char/bool
                _ => {
                    log::debug!("Unknown property type: 0x{value_type:02X}");
                    break;
                }
            }
        }

        None
    }
}
