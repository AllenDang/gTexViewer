use macroquad::math::Rect as MacroRect;
use macroquad::prelude::*;
use taffy::prelude::*;

use crate::texture_pipeline::EmbeddedMetadata;
use crate::types::{GTexViewerApp, ImageContext, ImageState};

pub fn image_measure_function(
    known_dimensions: Size<Option<f32>>,
    image_context: &ImageContext,
) -> Size<f32> {
    let aspect_ratio = image_context.width / image_context.height;

    let result = match (known_dimensions.width, known_dimensions.height) {
        (Some(width), Some(height)) => {
            // Both dimensions constrained - trust Taffy's layout algorithm
            Size { width, height }
        }
        (Some(width), None) => {
            // Width constrained, calculate height from aspect ratio
            let height = width / aspect_ratio;
            Size { width, height }
        }
        (None, Some(height)) => {
            // Height constrained, calculate width from aspect ratio
            let width = height * aspect_ratio;
            Size { width, height }
        }
        (None, None) => {
            // Unified thumbnail approach: standard size with correct aspect ratio
            let thumbnail_size = 100.0; // Standard thumbnail dimension

            if aspect_ratio >= 1.0 {
                // Landscape/square: constrain width, calculate height
                Size {
                    width: thumbnail_size,
                    height: thumbnail_size / aspect_ratio,
                }
            } else {
                // Portrait: constrain height, calculate width
                Size {
                    width: thumbnail_size * aspect_ratio,
                    height: thumbnail_size,
                }
            }
        }
    };

    // Check for aspect ratio distortion (reduced logging to prevent performance issues)
    let final_aspect = if result.height > 0.01 {
        result.width / result.height
    } else {
        0.0
    };
    if (final_aspect - aspect_ratio).abs() > 0.05 {
        // Increase threshold to reduce noise
        log::debug!("Measure distortion: {aspect_ratio:.3} -> {final_aspect:.3}");
    }

    // Ensure minimum dimensions to prevent invisible images
    Size {
        width: result.width.max(0.01),   // Minimum 0.01 world units
        height: result.height.max(0.01), // Minimum 0.01 world units
    }
}

impl GTexViewerApp {
    // Helper function to adjust metadata dimensions to aspect-ratio layout boxes (max 100px in world units)
    pub fn adjust_metadata_for_layout(metadata: &EmbeddedMetadata) -> EmbeddedMetadata {
        let original_width = metadata.width as f32;
        let original_height = metadata.height as f32;
        let max_size = 100.0; // This will be interpreted as pixels by the layout system

        let (layout_width, layout_height) = if original_width >= original_height {
            // Width-constrained: width = 100, height = proportional
            (max_size, max_size * original_height / original_width)
        } else {
            // Height-constrained: height = 100, width = proportional
            (max_size * original_width / original_height, max_size)
        };

        // Create new EmbeddedMetadata with adjusted dimensions but preserve the hints!
        let mut adjusted = metadata.clone(); // Use the existing Clone impl that handles hints
        adjusted.width = layout_width as usize;
        adjusted.height = layout_height as usize;
        adjusted
    }

    pub fn setup_layout(&mut self, available_size: Vec2) {
        if self.image_slots.is_empty() || !self.layout_needs_update {
            return;
        }

        log::debug!(
            "🔄 Recalculating layout for {} images",
            self.image_slots.len()
        );

        let slot_count = self.image_slots.len();

        // Special case for single image - use direct screen coordinates
        if slot_count == 1 {
            let slot = &mut self.image_slots[0];

            // Get the actual image size
            let image_size = match &slot.state {
                ImageState::Loaded { image } => {
                    vec2(image.info.width as f32, image.info.height as f32)
                }
                ImageState::Placeholder {
                    layout_metadata, ..
                } => vec2(layout_metadata.width as f32, layout_metadata.height as f32),
                ImageState::Failed { .. } => vec2(100.0, 100.0),
            };

            // In macroquad's camera system with zoom=(1, screen_width/screen_height):
            // - World width spans -1 to +1 (2 units total)
            // - World height spans -aspect to +aspect (2*aspect units total)
            let aspect_ratio = screen_width() / screen_height();
            let world_width = 2.0;
            let world_height = 2.0 * aspect_ratio;

            // Scale to fit 80% of current visible world space (considering zoom level)
            let visible_width = world_width / self.camera.zoom.x;
            let visible_height = world_height / self.camera.zoom.y;
            let max_width = visible_width * 0.8;
            let max_height = visible_height * 0.8;

            // Calculate scale based on visible world units at current zoom
            let scale_x = max_width / image_size.x;
            let scale_y = max_height / image_size.y;
            let scale = scale_x.min(scale_y);
            let display_size = image_size * scale;

            // Position at world origin for camera coordinates (0,0 is center)
            slot.position = vec2(-display_size.x * 0.5, -display_size.y * 0.5);
            slot.size = display_size;
        } else {
            // Use Taffy Flexbox for multi-image layout
            self.setup_taffy_flexbox_layout(available_size);
        }

        // Calculate actual content bounds based on all image positions
        self.calculate_content_bounds();

        self.layout_needs_update = false;
    }

    pub fn setup_taffy_flexbox_layout(&mut self, _available_size: Vec2) {
        // Clear existing tree
        self.taffy_tree = TaffyTree::new();

        // Calculate visible viewport space considering current zoom level
        // When zoomed out, we have more visible space and can fit more columns
        let base_viewport_width = screen_width();
        let base_viewport_height = screen_height();
        let viewport_width = base_viewport_width / self.camera.zoom.x;
        let viewport_height = base_viewport_height / self.camera.zoom.y;

        // Create flexbox container style that wraps items and centers them
        let gap_size = 20.0; // Gap in pixels
        let flex_style = Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: Some(JustifyContent::Center),
            align_content: Some(AlignContent::Center),
            align_items: Some(AlignItems::Center),
            size: Size {
                width: length(viewport_width),
                height: length(viewport_height),
            },
            gap: Size {
                width: length(gap_size),
                height: length(gap_size),
            },
            ..Default::default()
        };

        // Create nodes for each image slot using measure functions for aspect ratios
        let mut child_nodes = Vec::with_capacity(self.image_slots.len());

        for slot in self.image_slots.iter() {
            // Get the actual image size for the measure function
            let image_size = match &slot.state {
                ImageState::Loaded { image } => {
                    vec2(image.info.width as f32, image.info.height as f32)
                }
                ImageState::Placeholder {
                    layout_metadata, ..
                } => vec2(layout_metadata.width as f32, layout_metadata.height as f32),
                ImageState::Failed { .. } => vec2(100.0, 100.0),
            };

            // Create image context for measure function
            let image_context = ImageContext {
                width: image_size.x,
                height: image_size.y,
            };

            // Create child style that lets measure function and Taffy flexbox work together
            let child_style = Style {
                // Let measure function determine dimensions
                size: Size {
                    width: auto(),
                    height: auto(),
                },
                // No max_size constraints - let Taffy's flexbox algorithm handle space allocation
                flex_shrink: 1.0, // Allow shrinking if needed
                flex_grow: 0.0,   // Don't grow beyond measure function result
                ..Default::default()
            };

            // Create leaf node with context for measure function
            if let Ok(node) = self
                .taffy_tree
                .new_leaf_with_context(child_style, image_context)
            {
                child_nodes.push(node);
            }
        }

        // Create the flexbox container with all child nodes
        if let Ok(root_node) = self.taffy_tree.new_with_children(flex_style, &child_nodes) {
            // Compute layout with measure function
            // The container size should be the adjusted viewport size considering zoom!
            let container_size = Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            };

            let layout_result = self.taffy_tree.compute_layout_with_measure(
                root_node,
                container_size,
                |known_dimensions, _available_space, _node_id, node_context, _style| {
                    match node_context {
                        Some(context) => image_measure_function(known_dimensions, context),
                        None => Size::ZERO,
                    }
                },
            );

            if layout_result.is_ok() {
                // Apply computed layout to image slots, converting pixel coordinates to world coordinates
                for (index, slot) in self.image_slots.iter_mut().enumerate() {
                    if let Some(&child_node) = child_nodes.get(index)
                        && let Ok(layout) = self.taffy_tree.layout(child_node)
                    {
                        // Taffy gives us positions in visible space - convert to world coordinates
                        // The layout was computed using visible space dimensions, so we need to convert back
                        let pixels_per_world_unit =
                            base_viewport_width.max(base_viewport_height) / 2.0;
                        let world_scale = 1.0 / pixels_per_world_unit;

                        // Convert layout coordinates to world coordinates
                        let world_x = (layout.location.x - viewport_width / 2.0) * world_scale;
                        let world_y = (layout.location.y - viewport_height / 2.0) * world_scale;
                        let world_w = layout.size.width * world_scale;
                        let world_h = layout.size.height * world_scale;

                        slot.position = vec2(world_x, world_y);
                        slot.size = vec2(world_w, world_h);

                        // Debug logging for layout positions
                        log::debug!(
                            "Layout slot {index}: pos=({world_x:.1}, {world_y:.1}), size=({world_w:.1}, {world_h:.1})"
                        );
                    }
                }
            } else {
                log::error!("❌ Taffy layout computation failed: {layout_result:?}");
            }
        }
    }

    pub fn calculate_content_bounds(&mut self) {
        if self.image_slots.is_empty() {
            self.content_bounds = MacroRect::new(0.0, 0.0, 0.0, 0.0);
            return;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for slot in &self.image_slots {
            min_x = min_x.min(slot.position.x);
            min_y = min_y.min(slot.position.y);
            max_x = max_x.max(slot.position.x + slot.size.x);
            max_y = max_y.max(slot.position.y + slot.size.y);
        }

        self.content_bounds = MacroRect::new(min_x, min_y, max_x - min_x, max_y - min_y);
    }
}
