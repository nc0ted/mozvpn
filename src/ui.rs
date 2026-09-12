use crate::config::{load_config, save_config, AppConfig};
use crate::locations::{get_exit_host, probe_all_locations, Location, LocationPingCache, LOCATIONS};
use crate::platform::{self, disable_autostart, enable_autostart, is_autostart_enabled, SystemProxy, TrayHandle};
use crate::profile::auto_load_account_data;
use crate::proxy::{is_port_free, ProxyConfig, ProxyHealth, RunningProxy};
use eframe::egui::{self, text::{LayoutJob, TextFormat}, Color32, RichText, Rounding, Vec2, ViewportCommand};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use tokio::runtime::Runtime;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}

fn format_location_label(
    name: &str,
    code: &str,
    ping: Option<Option<u32>>,
    is_selected: bool,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &format!("{} ({})", name, code),
        0.0,
        TextFormat {
            color: if is_selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(210, 210, 220)
            },
            font_id: egui::FontId::proportional(12.0),
            ..Default::default()
        },
    );
    if let Some(res) = ping {
        let (ping_text, color) = match res {
            Some(ms) if ms < 80 => (format!("  {} ms", ms), Color32::from_rgb(74, 222, 128)),
            Some(ms) if ms < 160 => (format!("  {} ms", ms), Color32::from_rgb(250, 204, 21)),
            Some(ms) => (format!("  {} ms", ms), Color32::from_rgb(248, 113, 113)),
            None => ("  timeout".to_string(), Color32::from_rgb(140, 140, 150)),
        };
        job.append(
            &ping_text,
            0.0,
            TextFormat {
                color,
                font_id: egui::FontId::proportional(11.0),
                ..Default::default()
            },
        );
    }
    job
}

pub struct OutfoxApp {
    rt: Runtime,
    session_token: Zeroizing<String>,
    account_email: Option<String>,
    manual_token_mode: bool,
    selected_location_idx: usize,
    http_port_str: String,
    socks_port_str: String,
    system_proxy_enabled: bool,
    auto_connect: bool,
    start_with_system: bool,
    settings_open: bool,
    ping_cache: LocationPingCache,
    ping_rx: Option<Receiver<Vec<(&'static str, Option<u32>)>>>,
    connection_state: ConnectionState,
    proxy_handle: Option<RunningProxy>,
    connect_rx: Option<Receiver<Result<RunningProxy, String>>>,
    connect_task: Option<tokio::task::JoinHandle<()>>,
    port_check_rx: Option<Receiver<(bool, bool)>>,
    tray_handle: Option<TrayHandle>,
    quota_display: Option<String>,
    quota_rx: Option<Receiver<Option<String>>>,
    allow_lan: bool,
    start_minimized: bool,
    show_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    http_port_busy: bool,
    socks_port_busy: bool,
    initial_centered: bool,
    frames_to_hide: u8,
}

impl OutfoxApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        show_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
        start_minimized: bool,
    ) -> Self {
        if let Ok(mut guard) = repaint_ctx.lock() {
            *guard = Some(cc.egui_ctx.clone());
        }

        let rt = Runtime::new().expect("failed to create Tokio runtime");
        let (session_token, account_email) = match auto_load_account_data() {
            Ok(acc) => (Zeroizing::new(acc.session_token), acc.email),
            Err(e) => {
                crate::log_warn!("Firefox profile auto-load skipped: {:#}", e);
                (Zeroizing::new(String::new()), None)
            }
        };

        let cfg = load_config();
        let selected_location_idx = LOCATIONS
            .iter()
            .position(|l| l.code.eq_ignore_ascii_case(&cfg.location_code))
            .unwrap_or(0);

        let start_with_system = is_autostart_enabled();
        let tray_handle = platform::spawn_tray(&rt, cc.egui_ctx.clone());

        let (quota_tx, quota_rx) = channel();
        let quota_token = session_token.clone();
        if !quota_token.trim().is_empty() {
            rt.spawn(async move {
                match crate::auth::mint_pass(&quota_token).await {
                    Ok(pass) => {
                        let text = pass.quota.map(|q| q.format_display());
                        let _ = quota_tx.send(text);
                    }
                    Err(e) => {
                        crate::log_warn!("Failed to fetch initial quota: {:#}", e);
                    }
                }
            });
        }

        let (ping_tx, ping_rx) = channel();
        rt.spawn(async move {
            let res = probe_all_locations().await;
            let _ = ping_tx.send(res);
        });

        let mut app = Self {
            rt,
            session_token,
            account_email,
            manual_token_mode: false,
            selected_location_idx,
            http_port_str: cfg.http_port.to_string(),
            socks_port_str: cfg.socks_port.to_string(),
            system_proxy_enabled: cfg.system_proxy,
            auto_connect: cfg.auto_connect,
            start_with_system,
            start_minimized: cfg.start_minimized,
            allow_lan: cfg.allow_lan,
            show_flag,
            settings_open: false,
            ping_cache: LocationPingCache::new(),
            ping_rx: Some(ping_rx),
            connection_state: ConnectionState::Disconnected,
            proxy_handle: None,
            connect_rx: None,
            connect_task: None,
            port_check_rx: None,
            tray_handle,
            quota_display: None,
            quota_rx: Some(quota_rx),
            http_port_busy: cfg.http_port == 0 || !is_port_free(cfg.http_port),
            socks_port_busy: cfg.socks_port == 0 || !is_port_free(cfg.socks_port),
            initial_centered: false,
            frames_to_hide: if start_minimized { 2 } else { 0 },
        };

        if app.auto_connect && !app.session_token.trim().is_empty() {
            app.start_connecting();
        }

        app
    }

    fn update_port_status(&mut self) {
        let http_num = self.http_port_str.parse::<u16>().ok();
        let socks_num = self.socks_port_str.parse::<u16>().ok();
        self.http_port_busy = http_num.is_none_or(|p| !is_port_free(p));
        self.socks_port_busy = socks_num.is_none_or(|p| !is_port_free(p));
    }

    fn schedule_port_check(&mut self) {
        let http_num = self.http_port_str.parse::<u16>().ok();
        let socks_num = self.socks_port_str.parse::<u16>().ok();
        self.http_port_busy = http_num.is_none() || http_num == Some(0);
        self.socks_port_busy = socks_num.is_none() || socks_num == Some(0);

        if let (Some(h), Some(s)) = (http_num, socks_num) {
            if h != 0 && s != 0 {
                let (tx, rx) = channel();
                self.port_check_rx = Some(rx);
                self.rt.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    let h_busy = !is_port_free(h);
                    let s_busy = !is_port_free(s);
                    let _ = tx.send((h_busy, s_busy));
                });
            }
        }
    }

    fn save_current_config(&self) {
        let http_port = self.http_port_str.parse::<u16>().unwrap_or(2085);
        let socks_port = self.socks_port_str.parse::<u16>().unwrap_or(2081);
        let location_code = LOCATIONS
            .get(self.selected_location_idx)
            .map(|l| l.code.to_string())
            .unwrap_or_else(|| "us".to_string());

        save_config(&AppConfig {
            location_code,
            http_port,
            socks_port,
            system_proxy: self.system_proxy_enabled,
            auto_connect: self.auto_connect,
            start_with_system: self.start_with_system,
            start_minimized: self.start_minimized,
            allow_lan: self.allow_lan,
        });
    }

    fn start_connecting(&mut self) {
        if let Some(task) = self.connect_task.take() {
            task.abort();
        }
        if let Some(proxy) = self.proxy_handle.take() {
            proxy.stop();
        }
        if self.session_token.trim().is_empty() {
            if let Ok(acc) = auto_load_account_data() {
                self.session_token = Zeroizing::new(acc.session_token);
                self.account_email = acc.email;
            }
        }
        if self.session_token.trim().is_empty() {
            self.connection_state =
                ConnectionState::Error("No session token provided".to_string());
            return;
        }

        let http_port: u16 = self.http_port_str.parse().unwrap_or(2085);
        let socks_port: u16 = self.socks_port_str.parse().unwrap_or(2081);
        let location = &LOCATIONS[self.selected_location_idx];
        let exit_host = get_exit_host(location.code);
        let session_token = self.session_token.clone();

        self.connection_state = ConnectionState::Connecting;
        let (tx, rx) = channel();
        self.connect_rx = Some(rx);

        let config = ProxyConfig {
            session_token,
            exit_host,
            requested_http_port: http_port,
            requested_socks_port: socks_port,
            allow_lan: self.allow_lan,
        };

        let task = self.rt.spawn(async move {
            let res = RunningProxy::start(config).await.map_err(|e| {
                crate::log_error!("Proxy startup failed: {:#}", e);
                e.to_string()
            });
            let _ = tx.send(res);
        });
        self.connect_task = Some(task);
    }

    fn cancel_connecting(&mut self) {
        if let Some(task) = self.connect_task.take() {
            task.abort();
        }
        if let Some(rx) = self.connect_rx.take() {
            if let Ok(Ok(proxy)) = rx.try_recv() {
                proxy.stop();
            }
        }
        self.connection_state = ConnectionState::Disconnected;
        self.schedule_port_check();
    }

    fn disconnect(&mut self) {
        if let Some(task) = self.connect_task.take() {
            task.abort();
        }
        if let Some(proxy) = self.proxy_handle.take() {
            proxy.stop();
        }
        if self.system_proxy_enabled {
            let _ = SystemProxy::disable();
        }
        self.session_token.zeroize();
        self.connect_rx = None;
        self.connection_state = ConnectionState::Disconnected;
        self.schedule_port_check();
    }

    fn toggle_system_proxy(&mut self, enable: bool) {
        self.system_proxy_enabled = enable;
        if self.connection_state == ConnectionState::Connected {
            if let Some(proxy) = &self.proxy_handle {
                if enable {
                    if let Err(e) = SystemProxy::enable(proxy.http_port, proxy.socks_port) {
                        crate::log_error!("Failed to enable system proxy: {:#}", e);
                        self.system_proxy_enabled = false;
                    }
                } else if let Err(e) = SystemProxy::disable() {
                    crate::log_error!("Failed to disable system proxy: {:#}", e);
                }
            }
        }
    }

    fn check_async_connection_status(&mut self) {
        if let Some(rx) = &self.connect_rx {
            if let Ok(res) = rx.try_recv() {
                self.connect_task = None;
                match res {
                    Ok(server) => {
                        if self.connection_state != ConnectionState::Connecting {
                            server.stop();
                            return;
                        }
                        if self.system_proxy_enabled {
                            if let Err(e) = SystemProxy::enable(server.http_port, server.socks_port) {
                                crate::log_error!("Failed to enable system proxy: {:#}", e);
                                self.system_proxy_enabled = false;
                            }
                        }
                        self.proxy_handle = Some(server);
                        self.connection_state = ConnectionState::Connected;
                    }
                    Err(err) => {
                        if self.connection_state == ConnectionState::Connecting {
                            self.connection_state = ConnectionState::Error(err);
                        }
                    }
                }
                self.connect_rx = None;
            }
        }
    }
}

impl eframe::App for OutfoxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.frames_to_hide > 0 {
            self.frames_to_hide -= 1;
            platform::minimize_window(ctx);
            if self.frames_to_hide > 0 {
                ctx.request_repaint();
            }
        } else if !self.initial_centered {
            if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(cmd);
                self.initial_centered = true;
            }
        }

        self.check_async_connection_status();

        if let Some(proxy) = &self.proxy_handle {
            if let Ok(guard) = proxy.health.try_read() {
                match *guard {
                    ProxyHealth::Degraded => {
                        if self.connection_state == ConnectionState::Connected {
                            self.connection_state = ConnectionState::Reconnecting;
                        }
                    }
                    ProxyHealth::Healthy => {
                        if self.connection_state == ConnectionState::Reconnecting {
                            self.connection_state = ConnectionState::Connected;
                        }
                    }
                }
            }
        }

        if let Some(rx) = &self.ping_rx {
            if let Ok(results) = rx.try_recv() {
                self.ping_cache.update(results);
                self.ping_rx = None;
            }
        }

        if let Some(rx) = &self.port_check_rx {
            if let Ok((h_busy, s_busy)) = rx.try_recv() {
                self.http_port_busy = h_busy;
                self.socks_port_busy = s_busy;
                self.port_check_rx = None;
            }
        }

        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = Color32::from_rgb(14, 14, 18);
        visuals.panel_fill = Color32::from_rgb(14, 14, 18);
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(24, 24, 30);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 32, 40);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(44, 44, 54);
        visuals.widgets.active.bg_fill = Color32::from_rgb(56, 56, 68);
        ctx.set_visuals(visuals);

        let is_quitting = self
            .tray_handle
            .as_ref()
            .map(|t| t.quit_requested.load(Ordering::Relaxed))
            .unwrap_or(false);

        if is_quitting {
            self.disconnect();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        if ctx.input(|i| i.viewport().maximized == Some(true)) {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(290.0, 295.0)));
        }

        if self.show_flag.swap(false, Ordering::Relaxed) {
            self.frames_to_hide = 0;
            platform::restore_window(ctx);
            if !self.initial_centered {
                if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                    ctx.send_viewport_cmd(cmd);
                    self.initial_centered = true;
                }
            }
        }

        if let Some(tray) = &self.tray_handle {
            if tray.show_requested.swap(false, Ordering::Relaxed) {
                self.frames_to_hide = 0;
                platform::restore_window(ctx);
                if !self.initial_centered {
                    if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                        ctx.send_viewport_cmd(cmd);
                        self.initial_centered = true;
                    }
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            platform::minimize_window(ctx);
        }

        if let Some(rx) = &self.quota_rx {
            if let Ok(display) = rx.try_recv() {
                if display.is_some() {
                    self.quota_display = display;
                }
                self.quota_rx = None;
            }
        }

        if let Some(proxy) = &self.proxy_handle {
            if let Ok(guard) = proxy.current_pass.try_read() {
                if let Some(pass) = guard.as_ref() {
                    if let Some(quota) = &pass.quota {
                        self.quota_display = Some(quota.format_display());
                    }
                }
            }
        }

        let is_connected = matches!(
            self.connection_state,
            ConnectionState::Connected | ConnectionState::Reconnecting
        );
        let is_connecting = matches!(
            self.connection_state,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        );

        if is_connecting {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 5.0);

            let (header_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), egui::Sense::hover());
            let btn_size = Vec2::splat(20.0);
            let btn_y = header_rect.center().y - btn_size.y * 0.5;

            let close_rect = egui::Rect::from_min_size(
                egui::pos2(header_rect.max.x - btn_size.x - 4.0, btn_y),
                btn_size,
            );
            let min_rect = egui::Rect::from_min_size(
                egui::pos2(close_rect.min.x - btn_size.x - 4.0, btn_y),
                btn_size,
            );
            let settings_rect = egui::Rect::from_min_size(
                egui::pos2(min_rect.min.x - btn_size.x - 4.0, btn_y),
                btn_size,
            );

            let drag_start_x = if self.settings_open {
                let back_rect = egui::Rect::from_min_size(
                    egui::pos2(header_rect.min.x + 4.0, btn_y),
                    btn_size,
                );
                let back_resp = ui.interact(back_rect, ui.id().with("back_btn"), egui::Sense::click());
                if back_resp.hovered() {
                    let alpha = if back_resp.is_pointer_button_down_on() { 36 } else { 20 };
                    ui.painter().rect_filled(
                        back_rect,
                        Rounding::same(5.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                    );
                }
                let back_color = if back_resp.hovered() {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(160, 160, 172)
                };
                let bc = back_rect.center();
                let stroke = egui::Stroke::new(1.8_f32, back_color);
                ui.painter().line_segment([egui::pos2(bc.x + 1.5, bc.y - 4.5), egui::pos2(bc.x - 3.0, bc.y)], stroke);
                ui.painter().line_segment([egui::pos2(bc.x - 3.0, bc.y), egui::pos2(bc.x + 1.5, bc.y + 4.5)], stroke);
                if back_resp.clicked() {
                    self.settings_open = false;
                }
                back_rect.max.x + 4.0
            } else {
                header_rect.min.x
            };

            ui.painter().text(
                egui::pos2(header_rect.center().x, header_rect.center().y),
                egui::Align2::CENTER_CENTER,
                "OUTFOX",
                egui::FontId::proportional(19.0),
                Color32::from_rgb(255, 113, 57),
            );

            let drag_rect = egui::Rect::from_min_max(
                egui::pos2(drag_start_x, header_rect.min.y),
                egui::pos2(settings_rect.min.x - 4.0, header_rect.max.y),
            );
            let header_drag = ui.interact(drag_rect, ui.id().with("header_drag"), egui::Sense::click_and_drag());
            if header_drag.dragged() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            let settings_resp = ui.interact(settings_rect, ui.id().with("settings_btn"), egui::Sense::click());
            if settings_resp.hovered() || self.settings_open {
                let alpha = if settings_resp.is_pointer_button_down_on() { 36 } else { 20 };
                ui.painter().rect_filled(
                    settings_rect,
                    Rounding::same(5.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                );
            }
            let settings_color = if settings_resp.hovered() || self.settings_open {
                Color32::WHITE
            } else {
                Color32::from_rgb(160, 160, 172)
            };
            let sc = settings_rect.center();
            let stroke = egui::Stroke::new(1.4_f32, settings_color);
            ui.painter().line_segment([egui::pos2(sc.x - 4.5, sc.y - 4.0), egui::pos2(sc.x + 4.5, sc.y - 4.0)], stroke);
            ui.painter().circle_filled(egui::pos2(sc.x - 1.5, sc.y - 4.0), 1.8, settings_color);
            ui.painter().line_segment([egui::pos2(sc.x - 4.5, sc.y), egui::pos2(sc.x + 4.5, sc.y)], stroke);
            ui.painter().circle_filled(egui::pos2(sc.x + 1.8, sc.y), 1.8, settings_color);
            ui.painter().line_segment([egui::pos2(sc.x - 4.5, sc.y + 4.0), egui::pos2(sc.x + 4.5, sc.y + 4.0)], stroke);
            ui.painter().circle_filled(egui::pos2(sc.x - 2.0, sc.y + 4.0), 1.8, settings_color);
            if settings_resp.clicked() {
                self.settings_open = !self.settings_open;
            }

            let min_resp = ui.interact(min_rect, ui.id().with("min_btn"), egui::Sense::click());
            if min_resp.hovered() {
                let alpha = if min_resp.is_pointer_button_down_on() { 36 } else { 20 };
                ui.painter().rect_filled(
                    min_rect,
                    Rounding::same(5.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                );
            }
            let min_stroke_color = if min_resp.hovered() {
                Color32::WHITE
            } else {
                Color32::from_rgb(160, 160, 172)
            };
            let mid_y = min_rect.center().y;
            let half_w = 4.0;
            ui.painter().line_segment(
                [
                    egui::pos2(min_rect.center().x - half_w, mid_y),
                    egui::pos2(min_rect.center().x + half_w, mid_y),
                ],
                egui::Stroke::new(1.8_f32, min_stroke_color),
            );
            if min_resp.clicked() {
                platform::minimize_window(ctx);
            }

            let close_resp = ui.interact(close_rect, ui.id().with("close_btn"), egui::Sense::click());
            let close_center = close_rect.center();
            let close_arm = 3.0;
            if close_resp.hovered() {
                let bg_color = if close_resp.is_pointer_button_down_on() {
                    Color32::from_rgb(195, 38, 38)
                } else {
                    Color32::from_rgb(225, 48, 48)
                };
                ui.painter().rect_filled(close_rect, Rounding::same(5.0), bg_color);
                ui.painter().line_segment(
                    [
                        egui::pos2(close_center.x - close_arm, close_center.y - close_arm),
                        egui::pos2(close_center.x + close_arm, close_center.y + close_arm),
                    ],
                    egui::Stroke::new(1.8_f32, Color32::WHITE),
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(close_center.x + close_arm, close_center.y - close_arm),
                        egui::pos2(close_center.x - close_arm, close_center.y + close_arm),
                    ],
                    egui::Stroke::new(1.8_f32, Color32::WHITE),
                );
            } else {
                let stroke_color = Color32::from_rgb(235, 75, 75);
                ui.painter().line_segment(
                    [
                        egui::pos2(close_center.x - close_arm, close_center.y - close_arm),
                        egui::pos2(close_center.x + close_arm, close_center.y + close_arm),
                    ],
                    egui::Stroke::new(1.8_f32, stroke_color),
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(close_center.x + close_arm, close_center.y - close_arm),
                        egui::pos2(close_center.x - close_arm, close_center.y + close_arm),
                    ],
                    egui::Stroke::new(1.8_f32, stroke_color),
                );
            }
            if close_resp.clicked() {
                self.disconnect();
                std::process::exit(0);
            }

            if self.settings_open {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Settings")
                        .size(16.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(6.0);

                if ui.checkbox(&mut self.auto_connect, "Connect on launch").changed() {
                    self.save_current_config();
                }
                ui.add_space(4.0);

                if ui.checkbox(&mut self.start_with_system, "Start with system").changed() {
                    let res = if self.start_with_system {
                        enable_autostart()
                    } else {
                        disable_autostart()
                    };
                    if let Err(e) = res {
                        crate::log_warn!("Failed to update autostart setting: {:#}", e);
                        self.start_with_system = is_autostart_enabled();
                    }
                    self.save_current_config();
                }
                ui.add_space(4.0);

                if ui.checkbox(&mut self.start_minimized, "Start minimized").changed() {
                    self.save_current_config();
                }
                ui.add_space(4.0);

                if ui.checkbox(&mut self.allow_lan, "Allow LAN connections").changed() {
                    self.save_current_config();
                    if self.connection_state == ConnectionState::Connected {
                        if let Some(proxy) = self.proxy_handle.take() {
                            proxy.stop();
                        }
                        self.start_connecting();
                    }
                }
                if self.allow_lan {
                    if let Some(ip) = crate::proxy::get_local_ip() {
                        let http_port = self.http_port_str.trim();
                        let socks_port = self.socks_port_str.trim();
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!("LAN: {}:{} / {}:{}", ip, http_port, ip, socks_port))
                                .size(10.5)
                                .color(Color32::from_rgb(120, 180, 255)),
                        );
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                ui.label(
                    RichText::new("Outfox v0.2.0 · Powered by Fastly MASQUE")
                        .size(10.0)
                        .color(Color32::from_rgb(130, 130, 140)),
                );
                return;
            }

            ui.add_space(8.0);

            ui.vertical_centered(|ui| {
                let switch_size = Vec2::new(72.0, 38.0);
                let (rect, response) = ui.allocate_exact_size(switch_size, egui::Sense::click());

                let target_anim = if is_connected { 1.0 } else { 0.0 };
                let anim_progress = ctx.animate_value_with_time(ui.id().with("switch_anim"), target_anim, 0.18);

                let r_bg = egui::lerp(38.0..=255.0, anim_progress) as u8;
                let g_bg = egui::lerp(38.0..=113.0, anim_progress) as u8;
                let b_bg = egui::lerp(46.0..=57.0, anim_progress) as u8;

                let bg_color = if is_connecting {
                    let pulse = ((ctx.input(|i| i.time) * 6.0).sin() * 0.5 + 0.5) as f32;
                    let r = egui::lerp(180.0..=245.0, pulse) as u8;
                    let g = egui::lerp(90.0..=158.0, pulse) as u8;
                    Color32::from_rgb(r, g, 11)
                } else if response.hovered() {
                    if is_connected {
                        Color32::from_rgb(255, 130, 75)
                    } else {
                        Color32::from_rgb(52, 52, 62)
                    }
                } else {
                    Color32::from_rgb(r_bg, g_bg, b_bg)
                };

                ui.painter().rect_filled(rect, Rounding::same(19.0), bg_color);

                let knob_radius = 14.0;
                let knob_y = rect.center().y;
                let min_x = rect.min.x + 19.0;
                let max_x = rect.max.x - 19.0;

                let knob_x = if is_connecting {
                    let wave = (ctx.input(|i| i.time) * 4.0).sin() as f32 * 6.0;
                    rect.center().x + wave
                } else {
                    egui::lerp(min_x..=max_x, anim_progress)
                };

                ui.painter().circle_filled(
                    egui::pos2(knob_x, knob_y),
                    knob_radius,
                    Color32::WHITE,
                );

                if response.clicked() {
                    match &self.connection_state {
                        ConnectionState::Connected => {
                            self.disconnect();
                        }
                        ConnectionState::Connecting => {
                            self.cancel_connecting();
                        }
                        ConnectionState::Reconnecting => {
                            self.disconnect();
                        }
                        ConnectionState::Disconnected | ConnectionState::Error(_) => {
                            self.start_connecting();
                        }
                    }
                }

                ui.add_space(3.0);

                let (status_title, subtitle, title_color) = match &self.connection_state {
                    ConnectionState::Connected => (
                        "Connected",
                        "Traffic masked",
                        Color32::WHITE,
                    ),
                    ConnectionState::Connecting => (
                        "Connecting...",
                        "Establishing Fastly tunnel",
                        Color32::from_rgb(245, 158, 11),
                    ),
                    ConnectionState::Reconnecting => (
                        "Reconnecting...",
                        "Tunnel degraded, recovering",
                        Color32::from_rgb(245, 158, 11),
                    ),
                    ConnectionState::Disconnected => (
                        "Disconnected",
                        "Tap switch to protect traffic",
                        Color32::from_rgb(170, 170, 180),
                    ),
                    ConnectionState::Error(_) => (
                        "Error",
                        "Connection failed",
                        Color32::from_rgb(239, 68, 68),
                    ),
                };

                ui.label(RichText::new(status_title).size(17.0).strong().color(title_color));
                ui.label(
                    RichText::new(subtitle)
                        .size(10.5)
                        .color(Color32::from_rgb(130, 130, 140)),
                );
                if let Some(quota) = &self.quota_display {
                    ui.label(
                        RichText::new(quota)
                            .size(10.5)
                            .color(Color32::from_rgb(160, 160, 175)),
                    );
                }
            });

            ui.add_space(2.0);
            ui.separator();

            let current_location: &Location = &LOCATIONS[self.selected_location_idx];
            let current_ping = self.ping_cache.get(current_location.code);
            let current_label = format_location_label(
                current_location.name,
                current_location.code,
                current_ping,
                true,
            );

            let _combo_resp = egui::ComboBox::from_id_salt("location_selector")
                .selected_text(current_label)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    if !self.ping_cache.is_fresh() && self.ping_rx.is_none() {
                        let (tx, rx) = channel();
                        self.ping_rx = Some(rx);
                        self.rt.spawn(async move {
                            let res = probe_all_locations().await;
                            let _ = tx.send(res);
                        });
                    }
                    for (i, loc) in LOCATIONS.iter().enumerate() {
                        let is_selected = i == self.selected_location_idx;
                        let ping = self.ping_cache.get(loc.code);
                        let label = format_location_label(loc.name, loc.code, ping, is_selected);
                        if ui.selectable_label(is_selected, label).clicked() && !is_selected {
                            self.selected_location_idx = i;
                            self.save_current_config();
                            if let Some(proxy) = &self.proxy_handle {
                                let new_exit = loc.host.to_string();
                                proxy.change_exit(&self.rt, new_exit);
                            }
                        }
                    }
                });

            if self.ping_rx.is_some() {
                ctx.request_repaint();
            }

            let mut sysproxy_toggle = self.system_proxy_enabled;
            if ui
                .checkbox(&mut sysproxy_toggle, "Set as System Proxy")
                .changed()
            {
                self.toggle_system_proxy(sysproxy_toggle);
                self.save_current_config();
            }

            let http_busy = !is_connected && self.http_port_busy;
            let socks_busy = !is_connected && self.socks_port_busy;

            ui.horizontal(|ui| {
                ui.label(RichText::new("HTTP").size(11.0).color(Color32::from_rgb(150, 150, 160)));
                ui.add_space(2.0);
                let mut http_edit = egui::TextEdit::singleline(&mut self.http_port_str).desired_width(44.0);
                if http_busy {
                    http_edit = http_edit.text_color(Color32::from_rgb(248, 113, 113));
                }
                let resp = ui.add_enabled(!is_connected, http_edit);
                if resp.changed() {
                    self.save_current_config();
                    self.update_port_status();
                }
                if http_busy {
                    ui.painter().rect_stroke(resp.rect, 3.0f32, egui::Stroke::new(1.0f32, Color32::from_rgb(239, 68, 68)));
                    resp.on_hover_text("Port is busy or invalid");
                }

                ui.add_space(10.0);

                ui.label(RichText::new("SOCKS").size(11.0).color(Color32::from_rgb(150, 150, 160)));
                ui.add_space(2.0);
                let mut socks_edit = egui::TextEdit::singleline(&mut self.socks_port_str).desired_width(44.0);
                if socks_busy {
                    socks_edit = socks_edit.text_color(Color32::from_rgb(248, 113, 113));
                }
                let resp = ui.add_enabled(!is_connected, socks_edit);
                if resp.changed() {
                    self.save_current_config();
                    self.update_port_status();
                }
                if socks_busy {
                    ui.painter().rect_stroke(resp.rect, 3.0f32, egui::Stroke::new(1.0f32, Color32::from_rgb(239, 68, 68)));
                    resp.on_hover_text("Port is busy or invalid");
                }
            });

            if let Some(proxy) = &self.proxy_handle {
                let http_changed = proxy.http_port != proxy.requested_http_port;
                let socks_changed = proxy.socks_port != proxy.requested_socks_port;

                if http_changed || socks_changed {
                    let warn_msg = format!(
                        "Busy port! HTTP: {}, SOCKS: {}",
                        proxy.http_port, proxy.socks_port
                    );
                    ui.colored_label(Color32::from_rgb(234, 179, 8), warn_msg);
                }

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let exp = proxy.token_expiry_timestamp.load(Ordering::Relaxed);
                let seconds_left = if exp > now { (exp - now) as i64 } else { 0 };
                let exit_text = proxy.exit_host.try_read().map(|g| g.clone()).unwrap_or_default();

                ui.label(
                    RichText::new(format!("Exit: {} ({}s left)", exit_text, seconds_left))
                        .size(10.0)
                        .color(Color32::from_rgb(130, 130, 140)),
                );
            }

            ui.horizontal(|ui| {
                let email_display = self
                    .account_email
                    .as_deref()
                    .unwrap_or("No profile detected");
                ui.label(
                    RichText::new(email_display)
                        .size(10.5)
                        .color(Color32::from_rgb(140, 140, 150)),
                );
                ui.add_space(6.0);
                if ui
                    .small_button(if self.manual_token_mode {
                        "Hide"
                    } else {
                        "Edit"
                    })
                    .clicked()
                {
                    self.manual_token_mode = !self.manual_token_mode;
                }
            });

            if self.manual_token_mode {
                ui.add_enabled(
                    !is_connected,
                    egui::TextEdit::singleline(&mut *self.session_token)
                        .hint_text("Paste session token hex")
                        .desired_width(ui.available_width()),
                );
            }

            if let ConnectionState::Error(ref err) = self.connection_state {
                ui.colored_label(Color32::from_rgb(239, 68, 68), format!("Error: {}", err));
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_current_config();
        self.disconnect();
    }
}

impl Drop for OutfoxApp {
    fn drop(&mut self) {
        self.save_current_config();
        self.disconnect();
    }
}
