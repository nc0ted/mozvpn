pub mod autostart;
pub mod proxy;
pub mod tray;

use eframe::egui::{self, ViewportCommand};

pub fn init() {}

pub fn minimize_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
}

pub fn restore_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
}
