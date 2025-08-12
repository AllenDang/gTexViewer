use macroquad::prelude::*;

use crate::types::{ChannelMode, GTexViewerApp, HoveredImageInfo, ImageState};

impl GTexViewerApp {
    pub fn draw_ui(&mut self) {
        // Draw loading indicator if needed
        if self.is_loading && self.image_slots.is_empty() {
            let text = "Extracting image metadata...";
            let text_size = 24.0;
            let text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: text_size as u16,
                color: WHITE,
                ..Default::default()
            };
            let text_dims = measure_text(text, self.ui_font.as_ref(), text_size as u16, 1.0);
            let text_x = (screen_width() - text_dims.width) / 2.0;
            let text_y = (screen_height() + text_dims.height) / 2.0;
            draw_text_ex(text, text_x, text_y, text_params);
        } else if self.image_slots.is_empty() {
            // Draw main help message
            let main_text = "Drop image files here to load images";
            let main_text_size = 28.0;
            let main_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: main_text_size as u16,
                color: WHITE,
                ..Default::default()
            };
            let main_text_dims =
                measure_text(main_text, self.ui_font.as_ref(), main_text_size as u16, 1.0);
            let main_text_x = (screen_width() - main_text_dims.width) / 2.0;
            let main_text_y = (screen_height() + main_text_dims.height) / 2.0 - 30.0;
            draw_text_ex(main_text, main_text_x, main_text_y, main_text_params);

            // Draw supported formats info
            let formats_text = "Supports: PNG, JPEG, WebP, BMP, TIFF, GIF, FF, EXR, HDR, ICO, QOI, TGA, PNM, AVIF, KTX2, GLB/GLTF, FBX";
            let formats_text_size = 16.0;
            let formats_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: formats_text_size as u16,
                color: GRAY,
                ..Default::default()
            };
            let formats_text_dims = measure_text(
                formats_text,
                self.ui_font.as_ref(),
                formats_text_size as u16,
                1.0,
            );
            let formats_text_x = (screen_width() - formats_text_dims.width) / 2.0;
            let formats_text_y = main_text_y + 40.0;
            draw_text_ex(
                formats_text,
                formats_text_x,
                formats_text_y,
                formats_text_params,
            );

            // Draw controls info
            let controls_text = "Mouse: Drag to pan • Wheel: Zoom in/out • Keys: 1-8 for channel modes • C to cycle";
            let controls_text_size = 14.0;
            let controls_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: controls_text_size as u16,
                color: DARKGRAY,
                ..Default::default()
            };
            let controls_text_dims = measure_text(
                controls_text,
                self.ui_font.as_ref(),
                controls_text_size as u16,
                1.0,
            );
            let controls_text_x = (screen_width() - controls_text_dims.width) / 2.0;
            let controls_text_y = formats_text_y + 30.0;
            draw_text_ex(
                controls_text,
                controls_text_x,
                controls_text_y,
                controls_text_params,
            );
        }

        // Draw UI overlay with image count and zoom info if images are loaded
        if !self.image_slots.is_empty() {
            let loaded_count = self
                .image_slots
                .iter()
                .filter(|slot| matches!(slot.state, ImageState::Loaded { .. }))
                .count();
            let total_count = self.image_slots.len();

            let channel_mode_str = match self.channel_mode {
                ChannelMode::Normal => "RGBA",
                ChannelMode::Red => "Red",
                ChannelMode::Green => "Green",
                ChannelMode::Blue => "Blue",
                ChannelMode::Alpha => "Alpha",
                ChannelMode::SwapRG => "Swap R↔G",
                ChannelMode::SwapRB => "Swap R↔B",
                ChannelMode::SwapGB => "Swap G↔B",
            };

            let info_text = format!(
                "Images: {}/{} | Zoom: {:.1}x | Mode: {}",
                loaded_count, total_count, self.camera.zoom.x, channel_mode_str
            );
            let info_text_size = 16.0;

            // Draw semi-transparent background for text
            let text_dims = measure_text(
                &info_text,
                self.ui_font.as_ref(),
                info_text_size as u16,
                1.0,
            );
            let info_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: info_text_size as u16,
                color: WHITE,
                ..Default::default()
            };
            draw_rectangle(
                5.0,
                5.0,
                text_dims.width + 10.0,
                25.0,
                Color::new(0.0, 0.0, 0.0, 0.7),
            );
            draw_text_ex(&info_text, 10.0, 22.0, info_text_params);
        }

        // Draw hover image info panel
        if let Some(ref hover_info) = self.hovered_image_info {
            self.draw_hover_info_panel(hover_info);
        }
    }

    pub fn draw_hover_info_panel(&self, hover_info: &HoveredImageInfo) {
        let panel_padding = 10.0;
        let line_height = 18.0;
        let text_size = 14.0;

        // Prepare info lines
        let info_lines = [
            format!("File: {}", hover_info.file_name),
            format!("Size: {}", hover_info.dimensions),
            format!("Color: {}", hover_info.color_space),
            format!("File Size: {}", hover_info.file_size),
        ];

        // Calculate panel dimensions
        let max_text_width = info_lines
            .iter()
            .map(|line| measure_text(line, self.ui_font.as_ref(), text_size as u16, 1.0).width)
            .fold(0.0, f32::max);

        let panel_width = max_text_width + panel_padding * 2.0;
        let panel_height = info_lines.len() as f32 * line_height + panel_padding * 2.0;

        // Position panel relative to mouse, avoiding screen edges
        let mut panel_x = hover_info.mouse_pos.x + 15.0; // Offset from cursor
        let mut panel_y = hover_info.mouse_pos.y + 15.0;

        // Adjust if panel would go off screen
        if panel_x + panel_width > screen_width() {
            panel_x = hover_info.mouse_pos.x - panel_width - 15.0;
        }
        if panel_y + panel_height > screen_height() {
            panel_y = hover_info.mouse_pos.y - panel_height - 15.0;
        }

        // Ensure panel stays on screen
        panel_x = panel_x.max(5.0);
        panel_y = panel_y.max(5.0);

        // Round to pixel boundaries to prevent flickering and improve text clarity
        panel_x = panel_x.round();
        panel_y = panel_y.round();

        // Draw panel background with subtle border (avoid thin lines that flicker)
        draw_rectangle(
            panel_x,
            panel_y,
            panel_width,
            panel_height,
            Color::new(0.1, 0.1, 0.1, 0.95),
        );
        // Use thicker, darker border to reduce flickering
        draw_rectangle_lines(
            panel_x,
            panel_y,
            panel_width,
            panel_height,
            2.0,
            Color::new(0.3, 0.3, 0.3, 0.9),
        );

        // Draw info text with pixel-aligned positions
        for (i, line) in info_lines.iter().enumerate() {
            let text_x = (panel_x + panel_padding).round();
            let text_y = (panel_y + panel_padding + (i as f32 + 1.0) * line_height).round();
            let hover_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: text_size as u16,
                color: WHITE,
                ..Default::default()
            };
            draw_text_ex(line, text_x, text_y, hover_text_params);
        }
    }

    pub fn update_hover_info(&mut self) {
        let mouse_screen = mouse_position();
        let mouse_world = self.screen_to_world(vec2(mouse_screen.0, mouse_screen.1));

        // Find which image (if any) is under the mouse cursor
        self.hovered_image_info = None;

        for slot in self.image_slots.iter() {
            // Check if mouse is inside this image's bounds
            let left = slot.position.x;
            let right = slot.position.x + slot.size.x;
            let top = slot.position.y;
            let bottom = slot.position.y + slot.size.y;

            if mouse_world.x >= left
                && mouse_world.x <= right
                && mouse_world.y >= top
                && mouse_world.y <= bottom
            {
                match &slot.state {
                    ImageState::Loaded { image } => {
                        // Format file size in human readable format
                        let file_size_mb = image.info.file_size as f64 / (1024.0 * 1024.0);
                        let file_size_str = if file_size_mb >= 1.0 {
                            format!("{file_size_mb:.1} MB")
                        } else {
                            let file_size_kb_val = image.info.file_size as f64 / 1024.0;
                            format!("{file_size_kb_val:.1} KB")
                        };

                        // Extract filename from path
                        let file_name = image
                            .path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        self.hovered_image_info = Some(HoveredImageInfo {
                            file_name,
                            dimensions: format!("{}×{}", image.info.width, image.info.height),
                            file_size: file_size_str,
                            color_space: image.info.color_space.clone(),
                            mouse_pos: vec2(mouse_screen.0, mouse_screen.1),
                        });
                    }
                    ImageState::Placeholder {
                        original_metadata, ..
                    } => {
                        // Show info from original metadata while loading
                        let file_size_mb = original_metadata.file_size as f64 / (1024.0 * 1024.0);
                        let file_size_str = if file_size_mb >= 1.0 {
                            format!("{file_size_mb:.1} MB")
                        } else {
                            let file_size_kb_val = original_metadata.file_size as f64 / 1024.0;
                            format!("{file_size_kb_val:.1} KB")
                        };

                        // Extract filename from path
                        let file_name = original_metadata
                            .source_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        let status = "Loading...";

                        self.hovered_image_info = Some(HoveredImageInfo {
                            file_name,
                            dimensions: format!(
                                "{}×{}",
                                original_metadata.width, original_metadata.height
                            ),
                            file_size: file_size_str,
                            color_space: format!("{:?} ({})", original_metadata.format, status),
                            mouse_pos: vec2(mouse_screen.0, mouse_screen.1),
                        });
                    }
                    ImageState::Failed { metadata, error } => {
                        // Show basic info for failed images
                        let (file_name, dimensions, file_size) = if let Some(metadata) = metadata {
                            let file_name = metadata
                                .source_path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Unknown")
                                .to_string();
                            let dimensions = format!("{}×{}", metadata.width, metadata.height);
                            let file_size_mb = metadata.file_size as f64 / (1024.0 * 1024.0);
                            let file_size = if file_size_mb >= 1.0 {
                                format!("{file_size_mb:.1} MB")
                            } else {
                                let file_size_kb_val = metadata.file_size as f64 / 1024.0;
                                format!("{file_size_kb_val:.1} KB")
                            };
                            (file_name, dimensions, file_size)
                        } else {
                            (
                                "Failed to load".to_string(),
                                "Unknown".to_string(),
                                "Unknown".to_string(),
                            )
                        };

                        self.hovered_image_info = Some(HoveredImageInfo {
                            file_name,
                            dimensions,
                            file_size,
                            color_space: format!("Error: {error}"),
                            mouse_pos: vec2(mouse_screen.0, mouse_screen.1),
                        });
                    }
                }
                break; // Only show info for topmost image
            }
        }

        // Redraw will be automatically triggered by mouse_motion events
    }
}
