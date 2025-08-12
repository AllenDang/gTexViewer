use macroquad::prelude::*;
use std::env;

use gtexviewer::GTexViewerApp;

fn load_app_icon() -> Option<macroquad::miniquad::conf::Icon> {
    use image::{ImageFormat, imageops::FilterType};
    use std::io::Cursor;

    // Load the icon PNG file
    let icon_bytes = include_bytes!("..assets/icon/app.png");
    let img = image::load(Cursor::new(icon_bytes), ImageFormat::Png).ok()?;

    // Convert to RGBA format
    let rgba_img = img.to_rgba8();

    // Create different sizes for the icon
    let small_img = image::imageops::resize(&rgba_img, 16, 16, FilterType::Lanczos3);
    let medium_img = image::imageops::resize(&rgba_img, 32, 32, FilterType::Lanczos3);
    let big_img = image::imageops::resize(&rgba_img, 64, 64, FilterType::Lanczos3);

    // Convert to fixed-size arrays
    let mut small = [0u8; 16 * 16 * 4];
    let mut medium = [0u8; 32 * 32 * 4];
    let mut big = [0u8; 64 * 64 * 4];

    small.copy_from_slice(&small_img);
    medium.copy_from_slice(&medium_img);
    big.copy_from_slice(&big_img);

    Some(macroquad::miniquad::conf::Icon { small, medium, big })
}

fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: macroquad::miniquad::conf::Conf {
            window_title: "gTexViewer".to_owned(),
            window_width: 1024,
            window_height: 768,
            desktop_center: true,
            platform: macroquad::miniquad::conf::Platform {
                blocking_event_loop: true, // Enable power-saving mode
                ..Default::default()
            },
            icon: load_app_icon(), // Set custom app icon
            ..Default::default()
        },
        // Configure when to trigger updates in blocking event loop mode
        update_on: Some(macroquad::conf::UpdateTrigger {
            key_down: true,     // Trigger on key press (for channel switching)
            mouse_down: true,   // Trigger on mouse button press
            mouse_up: true,     // Trigger on mouse button release
            mouse_motion: true, // Trigger on mouse movement (for hover info)
            mouse_wheel: true,  // Trigger on mouse wheel (for zoom)
            touch: true,        // Trigger on touch events (mobile support)
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    env_logger::init();

    // Check if a file was passed as command line argument (for file association)
    let initial_file = env::args().nth(1);

    let mut app = GTexViewerApp::new(initial_file).await;

    loop {
        app.update().await;
        app.draw().await;

        next_frame().await;
    }
}
