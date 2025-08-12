use anyhow::Result;
use image::DynamicImage;
use imagesize::ImageType;
use macroquad::prelude::*;

use crate::texture_pipeline::{ImageDataParser, ImageInfo, LoadedImageData};

pub struct StandardFormat;

impl ImageDataParser for StandardFormat {
    fn can_parse(&self, data: &LoadedImageData) -> bool {
        matches!(
            data.format,
            ImageType::Png
                | ImageType::Jpeg
                | ImageType::Webp
                | ImageType::Bmp
                | ImageType::Tiff
                | ImageType::Gif
                | ImageType::Exr
                | ImageType::Heif(_)
                | ImageType::Hdr
                | ImageType::Ico
                | ImageType::Pnm
                | ImageType::Qoi
                | ImageType::Tga
                | ImageType::Farbfeld
        )
    }

    fn parse(&self, data: &LoadedImageData) -> Result<(Image, ImageInfo)> {
        let dynamic_image = match data.format {
            ImageType::Heif(_) => {
                // For HEIF/AVIF files, try to specify the format explicitly
                image::load_from_memory_with_format(&data.data, image::ImageFormat::Avif)
                    .or_else(|_| image::load_from_memory(&data.data))?
            }
            _ => image::load_from_memory(&data.data)?,
        };

        // Convert to macroquad Image
        let rgba_img = dynamic_image.to_rgba8();

        let (width, height) = rgba_img.dimensions();

        let macroquad_image = Image {
            width: rgba_img.width() as u16,
            height: rgba_img.height() as u16,
            bytes: rgba_img.into_raw(),
        };

        // Detect color space from the parsed image
        let color_space = self.detect_color_space(&dynamic_image);

        let info = ImageInfo {
            width,
            height,
            file_size: data.file_size as u64,
            color_space,
        };

        Ok((macroquad_image, info))
    }
}

impl StandardFormat {
    fn detect_color_space(&self, img: &DynamicImage) -> String {
        match img {
            DynamicImage::ImageLuma8(_) => "Grayscale",
            DynamicImage::ImageLumaA8(_) => "Grayscale + Alpha",
            DynamicImage::ImageRgb8(_) => "RGB",
            DynamicImage::ImageRgba8(_) => "RGBA",
            DynamicImage::ImageLuma16(_) => "Grayscale 16-bit",
            DynamicImage::ImageLumaA16(_) => "Grayscale + Alpha 16-bit",
            DynamicImage::ImageRgb16(_) => "RGB 16-bit",
            DynamicImage::ImageRgba16(_) => "RGBA 16-bit",
            DynamicImage::ImageRgb32F(_) => "RGB 32-bit Float",
            DynamicImage::ImageRgba32F(_) => "RGBA 32-bit Float",
            _ => "Unknown",
        }
        .to_string()
    }
}
