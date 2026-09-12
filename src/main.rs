#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod config;
mod locations;
mod logger;
mod platform;
mod profile;
mod proxy;
mod ui;

use std::sync::Arc;
use ui::OutfoxApp;

fn main() -> eframe::Result {
    let _ = rustls::crypto::ring::default_provider().install_default();
    platform::init();
    let _ = logger::init();
    log_info!("Starting Outfox on {}", std::env::consts::OS);

    let cfg = config::load_config();
    let args: Vec<String> = std::env::args().collect();
    let has_minimized_arg = args.iter().any(|a| a == "--minimized" || a == "-m");
    let start_minimized = cfg.start_minimized || has_minimized_arg;

    let show_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let repaint_ctx = Arc::new(std::sync::Mutex::new(None));
    if !platform::ensure_single_instance(!has_minimized_arg, Arc::clone(&show_flag), Arc::clone(&repaint_ctx)) {
        log_info!("Existing Outfox instance found, exiting");
        return Ok(());
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = platform::SystemProxy::disable();
        default_hook(info);
    }));

    let _ = ctrlc::set_handler(move || {
        let _ = platform::SystemProxy::disable();
        std::process::exit(0);
    });

    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon = if let Ok(image) = image::load_from_memory(icon_bytes) {
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        Some(Arc::new(eframe::egui::IconData {
            rgba: image.into_raw(),
            width,
            height,
        }))
    } else {
        None
    };

    let viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([290.0, 295.0])
        .with_min_inner_size([290.0, 295.0])
        .with_max_inner_size([290.0, 295.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_app_id("outfox")
        .with_title("Outfox");

    #[cfg(windows)]
    let viewport = viewport.with_decorations(false);

    let viewport = if let Some(i) = icon {
        viewport.with_icon(i)
    } else {
        viewport
    };

    let viewport = if start_minimized {
        viewport.with_visible(false)
    } else {
        viewport
    };

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Outfox",
        native_options,
        Box::new(move |cc| Ok(Box::new(OutfoxApp::new(cc, show_flag, repaint_ctx, start_minimized)))),
    );
    let _ = platform::SystemProxy::disable();
    result
}
