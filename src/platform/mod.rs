#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

pub use imp::proxy::SystemProxy;
pub use imp::{init, minimize_window, restore_window};

#[cfg(target_os = "linux")]
pub use linux::tray::TrayHandle;

#[cfg(target_os = "windows")]
pub use windows::tray::TrayHandle;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub struct TrayHandle {
    pub show_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub quit_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(target_os = "linux")]
pub fn spawn_tray(rt: &tokio::runtime::Runtime, repaint_ctx: eframe::egui::Context) -> Option<TrayHandle> {
    linux::tray::spawn_tray(rt, repaint_ctx)
}

#[cfg(target_os = "windows")]
pub fn spawn_tray(rt: &tokio::runtime::Runtime, repaint_ctx: eframe::egui::Context) -> Option<TrayHandle> {
    windows::tray::spawn_tray(rt, repaint_ctx)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn spawn_tray(_rt: &tokio::runtime::Runtime, _repaint_ctx: eframe::egui::Context) -> Option<TrayHandle> {
    None
}
