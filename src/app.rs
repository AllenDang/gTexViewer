use macroquad::prelude::*;
use std::path::PathBuf;

use crate::types::GTexViewerApp;

impl GTexViewerApp {
    pub async fn new(initial_file: Option<String>) -> Self {
        use crate::loading::AsyncImageLoader;
        use crate::types::ChannelMode;
        use macroquad::math::Rect as MacroRect;
        use taffy::prelude::TaffyTree;

        let mut app = Self {
            image_slots: Vec::new(),
            initial_file_path: None,
            metadata_receivers: Vec::new(),
            async_loader: AsyncImageLoader::new(),
            is_loading: false,
            layout_needs_update: true,
            camera: Camera2D::default(),
            newly_loaded: false,
            content_bounds: MacroRect::new(0.0, 0.0, 0.0, 0.0),
            loading_completed_once: false,
            taffy_tree: TaffyTree::new(),
            channel_switch_material: None,
            channel_mode: ChannelMode::Normal,
            hovered_image_info: None,
            ui_text_queue: Vec::new(),
            pending_metadata: Vec::new(),
            burst_render_until: Some(std::time::Instant::now() + std::time::Duration::from_secs(1)), // Force 1 second of rendering on startup
            ui_font: None,
            metadata_cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        // Load initial file if provided (from file association)
        if let Some(file_path) = initial_file {
            app.initial_file_path = Some(PathBuf::from(file_path));
        }

        // Initialize the channel switching shader
        app.init_channel_shader();

        // Load custom font
        app.load_ui_font();

        app
    }

    pub async fn update(&mut self) {
        // Check for burst rendering and trigger updates if needed
        if let Some(burst_until) = self.burst_render_until {
            if std::time::Instant::now() < burst_until {
                // Still in burst mode - trigger continuous updates
                macroquad::miniquad::window::schedule_update();
            } else {
                // Burst period ended
                self.burst_render_until = None;
                log::info!("🔋 Burst rendering period ended, returning to power-save mode");
            }
        }

        // Check for completed metadata extraction
        self.check_metadata_results();

        // Update async image loading from Rayon
        self.update_async_loading();

        // Load initial file if provided via command line
        self.load_initial_file_if_needed();

        // Handle drag and drop for multiple files
        self.handle_file_drops();

        // Handle camera input
        self.handle_camera_input();

        // Handle channel switching input
        self.handle_channel_input();

        // Handle layout recalculation input
        self.handle_layout_input();

        // Update hover info
        self.update_hover_info();
    }

    pub async fn draw(&mut self) {
        clear_background(BLACK);

        // Clear UI text queue for this frame
        self.ui_text_queue.clear();

        // Apply proper aspect ratio correction to our camera zoom
        let aspect_ratio = screen_width() / screen_height();
        let camera = Camera2D {
            zoom: vec2(self.camera.zoom.x, self.camera.zoom.y * aspect_ratio),
            target: self.camera.target,
            ..Default::default()
        };
        set_camera(&camera);

        // Draw all image slots (this populates ui_text_queue)
        self.draw_images();

        // Reset to default camera for UI (causes render pass flush)
        set_default_camera();

        // Render queued UI text from world coordinates
        for ui_text in &self.ui_text_queue {
            let ui_text_params = TextParams {
                font: self.ui_font.as_ref(),
                font_size: ui_text.size as u16,
                color: ui_text.color,
                ..Default::default()
            };
            draw_text_ex(&ui_text.text, ui_text.x, ui_text.y, ui_text_params);
        }

        // Draw UI elements
        self.draw_ui();
    }

    fn load_ui_font(&mut self) {
        // Embed Oswald font directly in the binary
        const OSWALD_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/Oswald-Regular.ttf");

        match load_ttf_font_from_bytes(OSWALD_FONT_BYTES) {
            Ok(font) => {
                log::info!("✅ Loaded Oswald UI font successfully");
                self.ui_font = Some(font);
            }
            Err(e) => {
                log::warn!("Failed to load Oswald font: {e}, using default font");
            }
        }
    }
}
