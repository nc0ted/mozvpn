pub mod proxy;

use eframe::egui::{self, ViewportCommand};

pub fn init() {}

pub fn minimize_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
}

pub fn restore_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
}
