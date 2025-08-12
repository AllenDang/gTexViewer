use anyhow::Result;
use imagesize::ImageType;
use macroquad::prelude::*;

use crate::texture_pipeline::{ImageDataParser, ImageInfo, LoadedImageData};

pub struct Ktx2Format;

impl ImageDataParser for Ktx2Format {
    fn can_parse(&self, data: &LoadedImageData) -> bool {
        data.format == ImageType::Ktx2
    }

    fn parse(&self, data: &LoadedImageData) -> Result<(Image, ImageInfo)> {
        // Parse KTX2 file
        let mut ktx2 = ktx2_rw::Ktx2Texture::from_memory(&data.data)?;

        let width = ktx2.width();
        let height = ktx2.height();

        // Transcode basis universal to RGBA8 if needed
        if ktx2.needs_transcoding() {
            ktx2.transcode_basis(ktx2_rw::TranscodeFormat::Rgba32)?;
        }

        // Get raw image data
        let image_data = ktx2.get_image_data(0, 0, 0)?;

        // Create macroquad Image from raw data
        let macroquad_image = Image {
            width: width as u16,
            height: height as u16,
            bytes: image_data.to_vec(),
        };

        let info = ImageInfo {
            width,
            height,
            file_size: data.file_size as u64,
            color_space: "RGBA".to_string(), // KTX2 transcoded to RGBA
        };

        Ok((macroquad_image, info))
    }
}
