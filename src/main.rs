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
use ui::MozVpnApp;

fn main() -> eframe::Result {
    let _ = rustls::crypto::ring::default_provider().install_default();
    platform::init();
    let _ = logger::init();
    log_info!("Starting MozVPN on {}", std::env::consts::OS);

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
        .with_inner_size([290.0, 285.0])
        .with_min_inner_size([280.0, 260.0])
        .with_max_inner_size([300.0, 300.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_app_id("mozvpn")
        .with_title("MozVPN");

    #[cfg(windows)]
    let viewport = viewport.with_decorations(false);

    let viewport = if let Some(i) = icon {
        viewport.with_icon(i)
    } else {
        viewport
    };

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let result = eframe::run_native(
        "MozVPN",
        native_options,
        Box::new(|cc| Ok(Box::new(MozVpnApp::new(cc)))),
    );
    let _ = platform::SystemProxy::disable();
    result
}
