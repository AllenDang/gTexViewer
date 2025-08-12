use macroquad::math::Rect as MacroRect;
use macroquad::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Instant;
use taffy::prelude::*;

use crate::loading::{AsyncImageLoader, LoadedImage};
use crate::texture_pipeline::EmbeddedMetadata;

#[derive(Clone)]
pub struct ImageContext {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelMode {
    Normal, // RGBA
    Red,    // Red channel only
    Green,  // Green channel only
    Blue,   // Blue channel only
    Alpha,  // Alpha channel only
    SwapRG, // Swap red and green channels
    SwapRB, // Swap red and blue channels
    SwapGB, // Swap green and blue channels
}

pub struct GTexViewerApp {
    pub image_slots: Vec<ImageSlot>,
    pub initial_file_path: Option<PathBuf>,
    pub metadata_receivers: Vec<mpsc::Receiver<MetadataResult>>,
    pub async_loader: AsyncImageLoader,
    pub is_loading: bool,
    pub layout_needs_update: bool,
    pub camera: Camera2D,
    pub newly_loaded: bool,
    pub content_bounds: MacroRect,
    pub loading_completed_once: bool, // Track if we've completed loading to avoid repeated auto-fit
    pub taffy_tree: TaffyTree<ImageContext>, // Layout engine
    pub channel_switch_material: Option<Material>, // Custom shader for RGBA channel switching
    pub channel_mode: ChannelMode,    // Current channel display mode
    pub hovered_image_info: Option<HoveredImageInfo>, // Info for image under mouse cursor
    pub ui_text_queue: Vec<UiText>,   // Queue UI text to minimize camera switches
    pub pending_metadata: Vec<EmbeddedMetadata>, // Store metadata until all arrive
    pub burst_render_until: Option<Instant>, // Force continuous rendering until this time
    pub ui_font: Option<Font>,        // Custom UI font
    pub metadata_cancel_flag: Arc<AtomicBool>, // Cancellation flag for metadata extraction
}

// Implement Drop to clean up resources when the app is destroyed
impl GTexViewerApp {
    /// Trigger burst rendering for a specified duration to ensure UI updates are visible
    pub fn start_burst_rendering(&mut self, duration: std::time::Duration) {
        let burst_until = std::time::Instant::now() + duration;
        self.burst_render_until = Some(burst_until);
        log::info!("⚡ Starting burst rendering for {duration:?}");
        // Immediately trigger an update
        macroquad::miniquad::window::schedule_update();
    }
}

impl Drop for GTexViewerApp {
    fn drop(&mut self) {
        log::info!("🔚 GTexViewerApp is being dropped, cleaning up resources");

        // Clean up GPU textures
        let mut cleaned_textures = 0;
        for slot in &mut self.image_slots {
            if let ImageState::Loaded { image: _ } = &slot.state {
                cleaned_textures += 1;
            }
        }

        if cleaned_textures > 0 {
            log::info!("🗑️ Cleaned up {cleaned_textures} GPU textures on app exit");
        }

        // Clear image slots to trigger texture cleanup
        self.image_slots.clear();
    }
}

pub type MetadataResult = Result<Vec<EmbeddedMetadata>, (PathBuf, String)>;

#[derive(Clone)]
pub enum ImageState {
    Placeholder {
        original_metadata: EmbeddedMetadata, // Keep original for hover info AND hints!
        layout_metadata: EmbeddedMetadata,   // Adjusted for layout (100x75, etc.) but keeps hints
    },
    Loaded {
        image: LoadedImage,
    },
    Failed {
        metadata: Option<EmbeddedMetadata>,
        error: String,
    },
}

pub struct ImageSlot {
    pub state: ImageState,
    pub position: Vec2,
    pub size: Vec2,
}

#[derive(Clone)]
pub struct HoveredImageInfo {
    pub file_name: String,
    pub dimensions: String,
    pub file_size: String,
    pub color_space: String,
    pub mouse_pos: Vec2, // Screen position for tooltip placement
}

#[derive(Debug, Clone)]
pub struct UiText {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: Color,
}
