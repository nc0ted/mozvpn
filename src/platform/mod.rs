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
pub use imp::autostart::{disable_autostart, enable_autostart, is_autostart_enabled};

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

#[cfg(unix)]
pub fn ensure_single_instance(
    request_focus: bool,
    show_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
) -> bool {
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};

    let socket_path = dirs::runtime_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("outfox.sock");

    if let Ok(mut stream) = UnixStream::connect(&socket_path) {
        if request_focus {
            let _ = stream.write_all(b"show\n");
            let _ = stream.flush();
        }
        return false;
    }

    let _ = std::fs::remove_file(&socket_path);
    if let Ok(listener) = UnixListener::bind(&socket_path) {
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buf = [0u8; 16];
                    if let Ok(n) = stream.read(&mut buf) {
                        if &buf[..n] == b"show\n" || &buf[..n] == b"show" {
                            show_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                            if let Ok(guard) = repaint_ctx.lock() {
                                if let Some(ctx) = guard.as_ref() {
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            }
        });
        true
    } else {
        true
    }
}

#[cfg(windows)]
pub fn ensure_single_instance(
    request_focus: bool,
    show_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    const SINGLE_INSTANCE_PORT: u16 = 49195;
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", SINGLE_INSTANCE_PORT)) {
        if request_focus {
            let _ = stream.write_all(b"show\n");
            let _ = stream.flush();
        }
        return false;
    }

    if let Ok(listener) = TcpListener::bind(("127.0.0.1", SINGLE_INSTANCE_PORT)) {
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buf = [0u8; 16];
                    if let Ok(n) = stream.read(&mut buf) {
                        if &buf[..n] == b"show\n" || &buf[..n] == b"show" {
                            show_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                            if let Ok(guard) = repaint_ctx.lock() {
                                if let Some(ctx) = guard.as_ref() {
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            }
        });
        true
    } else {
        true
    }
}

#[cfg(not(any(unix, windows)))]
pub fn ensure_single_instance(
    _request_focus: bool,
    _show_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
) -> bool {
    true
}

