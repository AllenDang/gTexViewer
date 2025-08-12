use macroquad::prelude::*;

use crate::types::{GTexViewerApp, ImageState};

impl GTexViewerApp {
    pub fn calculate_dynamic_zoom_limits(&self) -> (f32, f32) {
        if self.image_slots.is_empty() {
            return (0.1, 10.0);
        }

        let viewport_width = screen_width();
        let viewport_height = screen_height();

        // Calculate minimum zoom with reasonable lower bound
        let min_zoom = if self.content_bounds.w > 0.0 && self.content_bounds.h > 0.0 {
            let zoom_x = viewport_width / (self.content_bounds.w + 100.0); // Add padding
            let zoom_y = viewport_height / (self.content_bounds.h + 100.0);
            let fit_all_zoom = (zoom_x.min(zoom_y) * 0.8).max(0.01);

            // Allow reasonable zoom out range - either fit-all or 0.1x, whichever is smaller
            fit_all_zoom.min(0.1)
        } else {
            0.1
        };

        // Calculate maximum zoom based on highest resolution image
        let max_zoom = self.calculate_max_useful_zoom();

        (min_zoom, max_zoom)
    }

    pub fn calculate_max_useful_zoom(&self) -> f32 {
        let mut max_zoom: f32 = 5.0; // Default fallback

        for slot in &self.image_slots {
            if let ImageState::Loaded { image } = &slot.state {
                // Calculate zoom needed for 1:1 pixel mapping (pixel-perfect)
                // thumbnail_size_in_world_units * zoom * pixels_per_world_unit = original_pixels

                let thumbnail_width_world = slot.size.x;
                let thumbnail_height_world = slot.size.y;

                // Convert world units to screen pixels to find current scale
                let world_to_pixels_x = screen_width() / 2.0; // World spans -1 to +1 = 2 units
                let world_to_pixels_y = screen_height() / (2.0 * screen_height() / screen_width());

                let thumbnail_width_pixels = thumbnail_width_world * world_to_pixels_x;
                let thumbnail_height_pixels = thumbnail_height_world * world_to_pixels_y;

                // Zoom needed for 1:1 pixel mapping
                let zoom_for_1to1_x = image.info.width as f32 / thumbnail_width_pixels;
                let zoom_for_1to1_y = image.info.height as f32 / thumbnail_height_pixels;
                let zoom_for_1to1 = zoom_for_1to1_x.max(zoom_for_1to1_y);

                // Allow zooming well beyond 1:1 for detailed inspection
                // Large images need much higher zoom to reach pixel-perfect threshold
                let useful_zoom = zoom_for_1to1 * 10.0; // Increased from 4x to 10x

                max_zoom = max_zoom.max(useful_zoom);
            }
        }

        max_zoom.min(200.0) // Higher cap for large image pixel-perfect viewing
    }
}
