use anyhow::{Result, anyhow};
use imagesize::{AtcCompression, DdsCompression, ImageType, PkmCompression, PvrtcCompression};
use macroquad::prelude::*;

use crate::texture_pipeline::{ImageDataParser, ImageInfo, LoadedImageData};

pub struct CompressedFormat;

impl ImageDataParser for CompressedFormat {
    fn can_parse(&self, data: &LoadedImageData) -> bool {
        matches!(
            data.format,
            ImageType::Dds(_)
                | ImageType::Etc2(_)
                | ImageType::Eac(_)
                | ImageType::Pvrtc(_)
                | ImageType::Atc(_)
                | ImageType::Astc
        )
    }

    fn parse(&self, data: &LoadedImageData) -> Result<(Image, ImageInfo)> {
        let (rgba_data, color_space) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.decompress_texture(data)
            })) {
                Ok(result) => result?,
                Err(_) => {
                    return Err(anyhow!(
                        "Compressed texture decoder panicked for format {:?} ({}x{})",
                        data.format,
                        data.width,
                        data.height
                    ));
                }
            };

        let macroquad_image = Image {
            width: data.width as u16,
            height: data.height as u16,
            bytes: rgba_data,
        };

        let info = ImageInfo {
            width: data.width as u32,
            height: data.height as u32,
            file_size: data.file_size as u64,
            color_space,
        };

        Ok((macroquad_image, info))
    }
}

impl CompressedFormat {
    fn decompress_texture(&self, data: &LoadedImageData) -> Result<(Vec<u8>, String)> {
        let width = data.width;
        let height = data.height;

        // Validate dimensions to prevent overflow issues
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return Err(anyhow!("Invalid texture dimensions: {}x{}", width, height));
        }

        let mut rgba_buffer = vec![0u32; width * height];

        let color_space = match data.format {
            ImageType::Dds(compression) => {
                self.decompress_dds(&data.data, width, height, compression, &mut rgba_buffer)?
            }
            ImageType::Etc2(compression) | ImageType::Eac(compression) => {
                self.decompress_pkm(&data.data, width, height, compression, &mut rgba_buffer)?
            }
            ImageType::Pvrtc(compression) => {
                self.decompress_pvrtc(&data.data, width, height, compression, &mut rgba_buffer)?
            }
            ImageType::Atc(compression) => {
                self.decompress_atc(&data.data, width, height, compression, &mut rgba_buffer)?
            }
            ImageType::Astc => self.decompress_astc(&data.data, width, height, &mut rgba_buffer)?,
            _ => return Err(anyhow!("Unsupported compressed format")),
        };

        // Convert u32 RGBA to u8 bytes (RGBA format)
        // Some formats may have different channel ordering from texture2ddecoder
        let rgba_bytes: Vec<u8> = rgba_buffer
            .iter()
            .flat_map(|&pixel| {
                match data.format {
                    ImageType::Pvrtc(_)
                    | ImageType::Etc2(_)
                    | ImageType::Eac(_)
                    | ImageType::Dds(_) => {
                        // Most texture2ddecoder formats return BGRA, convert to RGBA
                        [
                            ((pixel >> 16) & 0xFF) as u8, // R (from B position)
                            ((pixel >> 8) & 0xFF) as u8,  // G
                            (pixel & 0xFF) as u8,         // B (from R position)
                            ((pixel >> 24) & 0xFF) as u8, // A
                        ]
                    }
                    _ => {
                        // Standard RGBA ordering for other formats
                        [
                            (pixel & 0xFF) as u8,         // R
                            ((pixel >> 8) & 0xFF) as u8,  // G
                            ((pixel >> 16) & 0xFF) as u8, // B
                            ((pixel >> 24) & 0xFF) as u8, // A
                        ]
                    }
                }
            })
            .collect();

        Ok((rgba_bytes, color_space))
    }

    fn decompress_dds(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        compression: DdsCompression,
        buffer: &mut [u32],
    ) -> Result<String> {
        match compression {
            DdsCompression::Bc1 => {
                texture2ddecoder::decode_bc1(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC1 decode error: {}", e))?;
                Ok("BC1 (DXT1)".to_string())
            }
            DdsCompression::Bc2 => Err(anyhow!("BC2 not supported by texture2ddecoder")),
            DdsCompression::Bc3 => {
                texture2ddecoder::decode_bc3(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC3 decode error: {}", e))?;
                Ok("BC3 (DXT5)".to_string())
            }
            DdsCompression::Bc4 => {
                texture2ddecoder::decode_bc4(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC4 decode error: {}", e))?;
                Ok("BC4 (ATI1)".to_string())
            }
            DdsCompression::Bc5 => {
                texture2ddecoder::decode_bc5(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC5 decode error: {}", e))?;
                Ok("BC5 (ATI2)".to_string())
            }
            DdsCompression::Bc6h => {
                texture2ddecoder::decode_bc6_unsigned(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC6H decode error: {}", e))?;
                Ok("BC6H (HDR)".to_string())
            }
            DdsCompression::Bc7 => {
                texture2ddecoder::decode_bc7(data, width, height, buffer)
                    .map_err(|e| anyhow!("BC7 decode error: {}", e))?;
                Ok("BC7".to_string())
            }
            DdsCompression::Rgba32 => {
                // Already uncompressed RGBA32
                if data.len() != width * height * 4 {
                    return Err(anyhow!("Invalid RGBA32 data size"));
                }
                for (i, pixel) in buffer.iter_mut().enumerate().take(width * height) {
                    let idx = i * 4;
                    let r = data[idx] as u32;
                    let g = data[idx + 1] as u32;
                    let b = data[idx + 2] as u32;
                    let a = data[idx + 3] as u32;
                    *pixel = r | (g << 8) | (b << 16) | (a << 24);
                }
                Ok("RGBA32".to_string())
            }
            DdsCompression::Rgb24 => {
                // Already uncompressed RGB24
                if data.len() != width * height * 3 {
                    return Err(anyhow!("Invalid RGB24 data size"));
                }
                for (i, pixel) in buffer.iter_mut().enumerate().take(width * height) {
                    let idx = i * 3;
                    let r = data[idx] as u32;
                    let g = data[idx + 1] as u32;
                    let b = data[idx + 2] as u32;
                    *pixel = r | (g << 8) | (b << 16) | (0xFF << 24); // Full alpha
                }
                Ok("RGB24".to_string())
            }
            DdsCompression::Unknown => Err(anyhow!("Unknown DDS compression format")),
        }
    }

    fn decompress_pkm(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        compression: PkmCompression,
        buffer: &mut [u32],
    ) -> Result<String> {
        match compression {
            PkmCompression::Etc1 => {
                texture2ddecoder::decode_etc1(data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC1 decode error: {}", e))?;
                Ok("ETC1".to_string())
            }
            PkmCompression::Etc2 => {
                texture2ddecoder::decode_etc2_rgb(data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGB decode error: {}", e))?;
                Ok("ETC2 RGB".to_string())
            }
            PkmCompression::Etc2A1 => {
                texture2ddecoder::decode_etc2_rgba1(data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGBA1 decode error: {}", e))?;
                Ok("ETC2 RGBA1".to_string())
            }
            PkmCompression::Etc2A8 => {
                texture2ddecoder::decode_etc2_rgba8(data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGBA8 decode error: {}", e))?;
                Ok("ETC2 RGBA8".to_string())
            }
            PkmCompression::EacR => {
                texture2ddecoder::decode_eacr(data, width, height, buffer)
                    .map_err(|e| anyhow!("EAC R decode error: {}", e))?;
                Ok("EAC R11".to_string())
            }
            PkmCompression::EacRg => {
                texture2ddecoder::decode_eacrg(data, width, height, buffer)
                    .map_err(|e| anyhow!("EAC RG decode error: {}", e))?;
                Ok("EAC RG11".to_string())
            }
            PkmCompression::EacRSigned => {
                texture2ddecoder::decode_eacr_signed(data, width, height, buffer)
                    .map_err(|e| anyhow!("EAC R11 Signed decode error: {}", e))?;
                Ok("EAC R11 Signed".to_string())
            }
            PkmCompression::EacRgSigned => {
                texture2ddecoder::decode_eacrg(data, width, height, buffer) // Note: texture2ddecoder doesn't have signed RG variant
                    .map_err(|e| anyhow!("EAC RG11 Signed decode error: {}", e))?;
                Ok("EAC RG11 Signed".to_string())
            }
            PkmCompression::Unknown => Err(anyhow!("Unknown PKM compression format")),
        }
    }

    fn decompress_pvrtc(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        compression: PvrtcCompression,
        buffer: &mut [u32],
    ) -> Result<String> {
        // Only true PVRTC formats require power-of-2 dimensions and minimum size
        match compression {
            PvrtcCompression::Pvrtc2BppRgb
            | PvrtcCompression::Pvrtc2BppRgba
            | PvrtcCompression::Pvrtc4BppRgb
            | PvrtcCompression::Pvrtc4BppRgba => {
                if !width.is_power_of_two() || !height.is_power_of_two() || width < 8 || height < 8
                {
                    return Err(anyhow!(
                        "PVRTC requires power-of-2 dimensions with minimum 8x8, got {}x{}",
                        width,
                        height
                    ));
                }
            }
            _ => {
                // ETC2/EAC formats don't have power-of-2 requirements
            }
        }

        // Skip PVRTC container header to get to the actual texture data for true PVRTC formats
        let texture_data = match compression {
            PvrtcCompression::Pvrtc2BppRgb
            | PvrtcCompression::Pvrtc2BppRgba
            | PvrtcCompression::Pvrtc4BppRgb
            | PvrtcCompression::Pvrtc4BppRgba => self.skip_pvrtc_header(data)?,
            _ => {
                // ETC2/EAC formats in PVR containers also need header skipping
                self.skip_pvrtc_header(data)?
            }
        };

        // Additional validation for PVRTC data size
        let expected_data_size = match compression {
            PvrtcCompression::Pvrtc2BppRgb | PvrtcCompression::Pvrtc2BppRgba => {
                (width * height) / 4 // 2 bits per pixel
            }
            PvrtcCompression::Pvrtc4BppRgb | PvrtcCompression::Pvrtc4BppRgba => {
                (width * height) / 2 // 4 bits per pixel
            }
            _ => texture_data.len(), // For non-PVRTC formats, use actual data length
        };

        match compression {
            PvrtcCompression::Pvrtc2BppRgb | PvrtcCompression::Pvrtc2BppRgba => {
                if texture_data.len() < expected_data_size {
                    return Err(anyhow!(
                        "PVRTC 2BPP data too small: got {} bytes, expected at least {}",
                        texture_data.len(),
                        expected_data_size
                    ));
                }
                texture2ddecoder::decode_pvrtc_2bpp(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("PVRTC 2BPP decode error: {}", e))?;
                Ok("PVRTC 2BPP".to_string())
            }
            PvrtcCompression::Pvrtc4BppRgb | PvrtcCompression::Pvrtc4BppRgba => {
                if texture_data.len() < expected_data_size {
                    return Err(anyhow!(
                        "PVRTC 4BPP data too small: got {} bytes, expected at least {}",
                        texture_data.len(),
                        expected_data_size
                    ));
                }
                texture2ddecoder::decode_pvrtc_4bpp(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("PVRTC 4BPP decode error: {}", e))?;
                Ok("PVRTC 4BPP".to_string())
            }
            PvrtcCompression::Etc2Rgb => {
                texture2ddecoder::decode_etc2_rgb(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGB decode error: {}", e))?;
                Ok("ETC2 RGB (in PVR)".to_string())
            }
            PvrtcCompression::Etc2Rgba => {
                texture2ddecoder::decode_etc2_rgba8(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGBA8 decode error: {}", e))?;
                Ok("ETC2 RGBA8 (in PVR)".to_string())
            }
            PvrtcCompression::Etc2RgbA1 => {
                texture2ddecoder::decode_etc2_rgba1(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("ETC2 RGBA1 decode error: {}", e))?;
                Ok("ETC2 RGBA1 (in PVR)".to_string())
            }
            PvrtcCompression::EacR11 => {
                texture2ddecoder::decode_eacr(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("EAC R11 decode error: {}", e))?;
                Ok("EAC R11 (in PVR)".to_string())
            }
            PvrtcCompression::EacRg11 => {
                texture2ddecoder::decode_eacrg(texture_data, width, height, buffer)
                    .map_err(|e| anyhow!("EAC RG11 decode error: {}", e))?;
                Ok("EAC RG11 (in PVR)".to_string())
            }
            PvrtcCompression::Unknown => Err(anyhow!("Unknown PVRTC compression format")),
        }
    }

    fn decompress_atc(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        compression: AtcCompression,
        buffer: &mut [u32],
    ) -> Result<String> {
        match compression {
            AtcCompression::Rgb => {
                texture2ddecoder::decode_atc_rgb4(data, width, height, buffer)
                    .map_err(|e| anyhow!("ATC RGB4 decode error: {}", e))?;
                Ok("ATC RGB4".to_string())
            }
            AtcCompression::RgbaExplicit => {
                texture2ddecoder::decode_atc_rgba8(data, width, height, buffer)
                    .map_err(|e| anyhow!("ATC RGBA8 Explicit decode error: {}", e))?;
                Ok("ATC RGBA8 Explicit".to_string())
            }
            AtcCompression::RgbaInterpolated => {
                texture2ddecoder::decode_atc_rgba8(data, width, height, buffer)
                    .map_err(|e| anyhow!("ATC RGBA8 Interpolated decode error: {}", e))?;
                Ok("ATC RGBA8 Interpolated".to_string())
            }
            AtcCompression::Unknown => Err(anyhow!("Unknown ATC compression format")),
        }
    }

    fn decompress_astc(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        buffer: &mut [u32],
    ) -> Result<String> {
        // ASTC requires block size information which we need to extract from the header
        // For now, we'll try common block sizes and detect which one works
        let common_block_sizes = [
            (4, 4),
            (5, 4),
            (5, 5),
            (6, 5),
            (6, 6),
            (8, 5),
            (8, 6),
            (8, 8),
            (10, 5),
            (10, 6),
            (10, 8),
            (10, 10),
            (12, 10),
            (12, 12),
        ];

        for (block_x, block_y) in common_block_sizes {
            if let Ok(()) =
                texture2ddecoder::decode_astc(data, width, height, block_x, block_y, buffer)
            {
                return Ok(format!("ASTC {block_x}x{block_y}"));
            }
        }

        Err(anyhow!("Failed to decode ASTC with any common block size"))
    }

    fn skip_pvrtc_header<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        // Check if this is a PVR v3 format file
        if data.len() < 4 {
            return Err(anyhow!("PVRTC data too small for header"));
        }

        if &data[0..4] == b"PVR\x03" {
            // PVR v3 format - header structure:
            // 0-3: Magic "PVR\x03"
            // 4-7: Flags (4 bytes)
            // 8-15: Pixel format (8 bytes)
            // 16-19: Colour space (4 bytes)
            // 20-23: Channel type (4 bytes)
            // 24-27: Height (4 bytes)
            // 28-31: Width (4 bytes)
            // 32-35: Depth (4 bytes)
            // 36-39: Number of surfaces (4 bytes)
            // 40-43: Number of faces (4 bytes)
            // 44-47: MIP map count (4 bytes)
            // 48-51: Meta data size (4 bytes)
            // 52+: Meta data (variable)
            // Then: Actual texture data

            if data.len() < 52 {
                return Err(anyhow!("PVRTC v3 data too small for complete header"));
            }

            // Read metadata size from offset 48-51 (little endian)
            let metadata_size =
                u32::from_le_bytes([data[48], data[49], data[50], data[51]]) as usize;

            let header_size = 52 + metadata_size;
            if data.len() < header_size {
                return Err(anyhow!(
                    "PVRTC v3 data too small for header + metadata: need {}, got {}",
                    header_size,
                    data.len()
                ));
            }

            Ok(&data[header_size..])
        } else {
            // Check for legacy format (header size usually 52)
            if data.len() >= 4 {
                let header_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                if header_size == 52 && data.len() >= header_size {
                    return Ok(&data[header_size..]);
                }
            }

            // If we can't identify the header format, return the data as-is
            // This maintains backward compatibility
            Ok(data)
        }
    }
}
