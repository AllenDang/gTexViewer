use macroquad::math::Rect as MacroRect;
use macroquad::prelude::*;

use crate::texture_pipeline::EmbeddedMetadata;
use crate::types::{ChannelMode, GTexViewerApp, ImageSlot, ImageState, UiText};

impl GTexViewerApp {
    pub fn init_channel_shader(&mut self) {
        const VERTEX_SHADER: &str = r"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec2 uv;
varying lowp vec4 color;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}";

        const FRAGMENT_SHADER: &str = r"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;

uniform sampler2D Texture;
uniform lowp int channel_mode;

void main() {
    lowp vec4 tex_color = texture2D(Texture, uv);
    
    if (channel_mode == 0) {
        // Normal RGBA
        gl_FragColor = tex_color * color;
    } else if (channel_mode == 1) {
        // Red channel only
        gl_FragColor = vec4(tex_color.r, tex_color.r, tex_color.r, tex_color.a) * color;
    } else if (channel_mode == 2) {
        // Green channel only
        gl_FragColor = vec4(tex_color.g, tex_color.g, tex_color.g, tex_color.a) * color;
    } else if (channel_mode == 3) {
        // Blue channel only
        gl_FragColor = vec4(tex_color.b, tex_color.b, tex_color.b, tex_color.a) * color;
    } else if (channel_mode == 4) {
        // Alpha channel only
        gl_FragColor = vec4(tex_color.a, tex_color.a, tex_color.a, 1.0) * color;
    } else if (channel_mode == 5) {
        // Swap red and green
        gl_FragColor = vec4(tex_color.g, tex_color.r, tex_color.b, tex_color.a) * color;
    } else if (channel_mode == 6) {
        // Swap red and blue
        gl_FragColor = vec4(tex_color.b, tex_color.g, tex_color.r, tex_color.a) * color;
    } else if (channel_mode == 7) {
        // Swap green and blue
        gl_FragColor = vec4(tex_color.r, tex_color.b, tex_color.g, tex_color.a) * color;
    } else {
        // Fallback to normal
        gl_FragColor = tex_color * color;
    }
}";

        let material = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX_SHADER,
                fragment: FRAGMENT_SHADER,
            },
            MaterialParams {
                uniforms: vec![UniformDesc::new("channel_mode", UniformType::Int1)],
                ..Default::default()
            },
        );

        match material {
            Ok(mat) => {
                self.channel_switch_material = Some(mat);
            }
            Err(e) => {
                log::error!("Failed to load channel switching shader: {e}");
            }
        }
    }

    pub fn draw_images(&mut self) {
        // Setup layout if needed
        let available_size = vec2(screen_width(), screen_height());
        self.setup_layout(available_size);

        // Auto-fit camera for newly loaded images
        if self.newly_loaded {
            // Reset camera to default state
            self.camera.target = vec2(0.0, 0.0);

            // Calculate appropriate zoom level based on content
            let zoom = self.calculate_initial_zoom();
            self.camera.zoom = vec2(zoom, zoom);

            self.newly_loaded = false;
        }

        // Collect UI texts to avoid borrowing conflicts
        let mut ui_texts = Vec::new();

        // Draw all image slots at their calculated positions
        for slot in self.image_slots.iter() {
            match &slot.state {
                ImageState::Placeholder {
                    original_metadata, ..
                } => {
                    let mut placeholder_texts = self.draw_placeholder(slot, original_metadata);
                    ui_texts.append(&mut placeholder_texts);
                }

                ImageState::Loaded { image } => {
                    // Determine filtering mode based on zoom level and set it on the texture
                    let use_pixel_perfect = self.should_use_pixel_perfect_for_slot(slot);
                    let filter_mode = if use_pixel_perfect {
                        FilterMode::Nearest
                    } else {
                        FilterMode::Linear
                    };

                    // Apply filtering mode to the texture at render time
                    image.texture.set_filter(filter_mode);

                    // Use custom shader if available and channel mode is not normal
                    if let Some(ref material) = self.channel_switch_material
                        && self.channel_mode != ChannelMode::Normal
                    {
                        // Set the channel mode uniform
                        let mode_value = match self.channel_mode {
                            ChannelMode::Normal => 0,
                            ChannelMode::Red => 1,
                            ChannelMode::Green => 2,
                            ChannelMode::Blue => 3,
                            ChannelMode::Alpha => 4,
                            ChannelMode::SwapRG => 5,
                            ChannelMode::SwapRB => 6,
                            ChannelMode::SwapGB => 7,
                        };

                        material.set_uniform("channel_mode", mode_value);
                        gl_use_material(material);
                    }

                    draw_texture_ex(
                        &image.texture,
                        slot.position.x,
                        slot.position.y,
                        WHITE, // Use WHITE for normal texture rendering
                        DrawTextureParams {
                            dest_size: Some(slot.size),
                            ..Default::default()
                        },
                    );

                    // Reset to default material if we used custom shader
                    if self.channel_switch_material.is_some()
                        && self.channel_mode != ChannelMode::Normal
                    {
                        gl_use_default_material();
                    }
                }
                ImageState::Failed {
                    metadata: _,
                    error: _,
                } => {
                    // Draw simple error placeholder like the original
                    let rect =
                        MacroRect::new(slot.position.x, slot.position.y, slot.size.x, slot.size.y);
                    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, RED);

                    // Use macroquad's built-in coordinate conversion
                    let center_world = vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                    let center_screen = self.camera.world_to_screen(center_world);

                    // Store text for UI rendering pass (avoid frequent camera switches)
                    let text = "Error";
                    let text_size = 20.0;
                    let text_dims =
                        measure_text(text, self.ui_font.as_ref(), text_size as u16, 1.0);
                    let text_x = center_screen.x - text_dims.width / 2.0;
                    let text_y = center_screen.y + text_dims.height / 2.0;

                    // Add to UI text collection
                    ui_texts.push(UiText {
                        text: text.to_string(),
                        x: text_x,
                        y: text_y,
                        size: text_size,
                        color: RED,
                    });
                }
            }
        }

        // Add collected UI texts to queue
        self.ui_text_queue.extend(ui_texts);
    }

    pub fn draw_placeholder(&self, slot: &ImageSlot, metadata: &EmbeddedMetadata) -> Vec<UiText> {
        // Use the original simple loading placeholder - just like the original working system
        let rect = MacroRect::new(slot.position.x, slot.position.y, slot.size.x, slot.size.y);

        // Debug: Check if placeholder has valid size/position
        if rect.w <= 0.001 || rect.h <= 0.001 {
            // Don't spam - only log first few
            if rect.w == 0.0 && rect.h == 0.0 {
                log::debug!(
                    "Placeholder for {} waiting for layout: pos=({:.3}, {:.3}), size=({:.3}, {:.3})",
                    metadata.name,
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h
                );
            }
            return vec![];
        }

        // Draw placeholder border using individual lines for better control
        let line_thickness = 0.004; // Slightly thicker for visibility
        let color = Color::new(0.8, 0.8, 0.8, 0.9); // Light gray, slightly transparent

        // Draw four separate lines to ensure all borders are visible
        // Top line
        draw_line(
            rect.x,
            rect.y,
            rect.x + rect.w,
            rect.y,
            line_thickness,
            color,
        );
        // Bottom line
        draw_line(
            rect.x,
            rect.y + rect.h,
            rect.x + rect.w,
            rect.y + rect.h,
            line_thickness,
            color,
        );
        // Left line
        draw_line(
            rect.x,
            rect.y,
            rect.x,
            rect.y + rect.h,
            line_thickness,
            color,
        );
        // Right line
        draw_line(
            rect.x + rect.w,
            rect.y,
            rect.x + rect.w,
            rect.y + rect.h,
            line_thickness,
            color,
        );

        // Draw loading spinner directly in world coordinates (same layer as border)
        let center_world = vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);

        // Create a simple rotating spinner
        let time = get_time() as f32;
        let rotation = time * 3.0; // Rotate 3 radians per second

        // Fixed spinner size for all placeholders
        let spinner_radius = 0.02; // Fixed size in world coordinates
        let line_thickness = 0.006; // Fixed line thickness

        // Draw spinner as rotating lines
        let num_lines = 8;
        for i in 0..num_lines {
            let angle = rotation + (i as f32) * std::f32::consts::PI * 2.0 / (num_lines as f32);
            let alpha = (i as f32) / (num_lines as f32); // Fade effect
            let color = Color::new(1.0, 1.0, 1.0, alpha * 0.8 + 0.2);

            let start_radius = spinner_radius * 0.3;
            let end_radius = spinner_radius;

            let start_x = center_world.x + angle.cos() * start_radius;
            let start_y = center_world.y + angle.sin() * start_radius;
            let end_x = center_world.x + angle.cos() * end_radius;
            let end_y = center_world.y + angle.sin() * end_radius;

            draw_line(start_x, start_y, end_x, end_y, line_thickness, color);
        }

        // Return empty vector since we drew directly
        vec![]
    }

    pub fn calculate_initial_zoom(&self) -> f32 {
        // If no images loaded, use default zoom
        if self.image_slots.is_empty() {
            return 1.0;
        }

        // For single image, zoom to fit it comfortably on screen
        if self.image_slots.len() == 1 {
            let slot = &self.image_slots[0];

            // Get the actual image dimensions
            let image_size = match &slot.state {
                ImageState::Loaded { image } => {
                    vec2(image.info.width as f32, image.info.height as f32)
                }
                ImageState::Placeholder {
                    original_metadata, ..
                } => vec2(
                    original_metadata.width as f32,
                    original_metadata.height as f32,
                ),
                ImageState::Failed { .. } => return 1.0,
            };

            // Calculate how much screen space the image takes up in world coordinates
            // The layout system already positioned the image to fit 80% of world space
            let aspect_ratio = screen_width() / screen_height();
            let world_width = 2.0; // World spans -1 to +1
            let world_height = 2.0 * aspect_ratio;

            // Calculate what zoom level makes the image visible and comfortable to view
            // The layout system scales to fit 80% of world space, so at zoom=1.0 the image
            // should already be sized appropriately. However, for very large or small images,
            // we might want to adjust this.

            let max_world_width = world_width * 0.9; // Use 90% of available width
            let max_world_height = world_height * 0.9; // Use 90% of available height

            // Calculate the zoom needed to fit the original image size into screen space
            let scale_x = max_world_width / image_size.x;
            let scale_y = max_world_height / image_size.y;
            let fit_scale = scale_x.min(scale_y);

            // The zoom is the factor to make this comfortable to view
            // Since layout already handles sizing, we mainly need to ensure visibility
            if fit_scale < 0.1 {
                // For very large images, zoom out more
                0.5
            } else if fit_scale > 10.0 {
                // For very small images, zoom in more
                2.0
            } else {
                // For reasonably sized images, use a zoom that makes them comfortable to view
                (fit_scale * 100.0).clamp(0.5, 3.0)
            }
        } else {
            // For multiple images, use default zoom to show the layout
            1.0
        }
    }

    pub fn should_use_pixel_perfect_for_slot(&self, slot: &ImageSlot) -> bool {
        match &slot.state {
            ImageState::Loaded { image } => {
                // Calculate the actual zoom level needed for 1:1 pixel mapping
                // thumbnail_size_in_world_units * zoom * pixels_per_world_unit = original_pixels

                let thumbnail_width_world = slot.size.x;
                let thumbnail_height_world = slot.size.y;

                // Convert world units to screen pixels to find current scale
                let aspect_ratio = screen_width() / screen_height();
                let world_to_pixels_x = screen_width() / 2.0; // World spans -1 to +1 = 2 units
                let world_to_pixels_y = screen_height() / (2.0 * aspect_ratio);

                let thumbnail_width_pixels =
                    thumbnail_width_world * world_to_pixels_x * self.camera.zoom.x;
                let thumbnail_height_pixels =
                    thumbnail_height_world * world_to_pixels_y * self.camera.zoom.y;

                // Check if we're at or above 1:1 pixel mapping (pixel-perfect threshold)
                let scale_x = thumbnail_width_pixels / image.info.width as f32;
                let scale_y = thumbnail_height_pixels / image.info.height as f32;
                let effective_scale = scale_x.max(scale_y);

                // Use pixel-perfect when at 0.5x or higher scale (easier to trigger for large images)
                effective_scale >= 0.5
            }
            _ => false,
        }
    }
}
