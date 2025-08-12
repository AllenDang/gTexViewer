use macroquad::prelude::*;

use crate::types::{ChannelMode, GTexViewerApp};

impl GTexViewerApp {
    pub fn handle_camera_input(&mut self) {
        // Handle mouse wheel for zoom at cursor position
        let wheel = mouse_wheel().1;
        if wheel != 0.0 {
            let zoom_factor = 1.015_f32.powf(wheel); // Very low sensitivity for precise zoom control

            // Get mouse position in screen coordinates
            let mouse_screen = mouse_position();

            // Convert mouse screen position to world coordinates BEFORE zoom
            let world_point_before_zoom =
                self.screen_to_world(vec2(mouse_screen.0, mouse_screen.1));

            // Apply zoom with limits
            let new_zoom = self.camera.zoom * zoom_factor;
            let (min_zoom, max_zoom) = self.calculate_dynamic_zoom_limits();
            let clamped_zoom = vec2(
                new_zoom.x.clamp(min_zoom, max_zoom),
                new_zoom.y.clamp(min_zoom, max_zoom),
            );

            // Calculate actual zoom factor that was applied (in case it was clamped)
            let actual_zoom_factor = clamped_zoom.x / self.camera.zoom.x;

            // Only adjust camera if zoom actually changed
            if (actual_zoom_factor - 1.0).abs() > 0.001 {
                self.camera.zoom = clamped_zoom;

                // Convert same mouse screen position to world coordinates AFTER zoom
                let world_point_after_zoom =
                    self.screen_to_world(vec2(mouse_screen.0, mouse_screen.1));

                // Adjust camera target so the world point under cursor stays the same
                let world_offset = world_point_before_zoom - world_point_after_zoom;
                self.camera.target += world_offset;

                // Redraw will be automatically triggered by mouse_wheel event
            }
        }

        // Handle mouse drag for pan - sensitivity adjusted by zoom level
        if is_mouse_button_down(MouseButton::Left) {
            let mouse_delta = mouse_delta_position();

            // Base sensitivity that feels natural at 1x zoom
            let base_sensitivity = 1.0;

            // Adjust sensitivity inversely with zoom: higher zoom = lower sensitivity
            // This makes panning feel consistent regardless of zoom level
            let zoom_adjusted_sensitivity = base_sensitivity / self.camera.zoom.x;

            let world_delta = vec2(
                mouse_delta.x * zoom_adjusted_sensitivity,
                mouse_delta.y * zoom_adjusted_sensitivity,
            );

            // Direct addition for natural movement: drag right = image moves right
            self.camera.target += world_delta;

            // Redraw will be automatically triggered by mouse_down/mouse_motion events
        }
    }

    pub fn handle_channel_input(&mut self) {
        // Cycle through channel modes with number keys
        if is_key_pressed(KeyCode::Key1) {
            self.channel_mode = ChannelMode::Normal;
        } else if is_key_pressed(KeyCode::Key2) {
            self.channel_mode = ChannelMode::Red;
        } else if is_key_pressed(KeyCode::Key3) {
            self.channel_mode = ChannelMode::Green;
        } else if is_key_pressed(KeyCode::Key4) {
            self.channel_mode = ChannelMode::Blue;
        } else if is_key_pressed(KeyCode::Key5) {
            self.channel_mode = ChannelMode::Alpha;
        } else if is_key_pressed(KeyCode::Key6) {
            self.channel_mode = ChannelMode::SwapRG;
        } else if is_key_pressed(KeyCode::Key7) {
            self.channel_mode = ChannelMode::SwapRB;
        } else if is_key_pressed(KeyCode::Key8) {
            self.channel_mode = ChannelMode::SwapGB;
        }

        // Or use C key to cycle through modes
        if is_key_pressed(KeyCode::C) {
            self.channel_mode = match self.channel_mode {
                ChannelMode::Normal => ChannelMode::Red,
                ChannelMode::Red => ChannelMode::Green,
                ChannelMode::Green => ChannelMode::Blue,
                ChannelMode::Blue => ChannelMode::Alpha,
                ChannelMode::Alpha => ChannelMode::SwapRG,
                ChannelMode::SwapRG => ChannelMode::SwapRB,
                ChannelMode::SwapRB => ChannelMode::SwapGB,
                ChannelMode::SwapGB => ChannelMode::Normal,
            };
        }

        // Redraw will be automatically triggered by key_down events
    }

    pub fn handle_layout_input(&mut self) {
        if is_key_pressed(KeyCode::R) {
            log::info!("🔄 Recalculating layout to fit viewport at current zoom level");
            self.layout_needs_update = true;
        }
    }

    pub fn screen_to_world(&self, screen_pos: Vec2) -> Vec2 {
        // Convert screen coordinates to world coordinates using camera transform
        let screen_width = screen_width();
        let screen_height = screen_height();

        // Normalize screen position to [-1, 1] range
        let normalized_x = (screen_pos.x / screen_width) * 2.0 - 1.0;
        let normalized_y = (screen_pos.y / screen_height) * 2.0 - 1.0;

        // Apply camera transform matching exactly how camera is set up in draw()
        // In draw(): zoom.y is multiplied by aspect_ratio
        let aspect_ratio = screen_width / screen_height;
        let effective_zoom_y = self.camera.zoom.y * aspect_ratio;

        let world_x = self.camera.target.x + normalized_x / self.camera.zoom.x;
        let world_y = self.camera.target.y + normalized_y / effective_zoom_y;

        vec2(world_x, world_y)
    }
}
