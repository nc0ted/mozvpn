use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayHandle {
    pub show_requested: Arc<AtomicBool>,
    pub quit_requested: Arc<AtomicBool>,
    _tray_icon: TrayIcon,
}

pub fn spawn_tray(_rt: &tokio::runtime::Runtime, repaint_ctx: egui::Context) -> Option<TrayHandle> {
    let show_requested = Arc::new(AtomicBool::new(false));
    let quit_requested = Arc::new(AtomicBool::new(false));

    let icon_bytes = include_bytes!("../../../assets/icon.png");
    let (rgba, width, height) = if let Ok(image) = image::load_from_memory(icon_bytes) {
        let image = image.to_rgba8();
        let (w, h) = image.dimensions();
        (image.into_raw(), w, h)
    } else {
        return None;
    };
    let icon = Icon::from_rgba(rgba, width, height).ok()?;

    let menu = Menu::new();
    let show_item = MenuItem::new("Show Outfox", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("Quit", true, None);

    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let _ = menu.append_items(&[
        &show_item as &dyn IsMenuItem,
        &separator as &dyn IsMenuItem,
        &quit_item as &dyn IsMenuItem,
    ]);

    let tray_icon = match TrayIconBuilder::new()
        .with_tooltip("Outfox")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(icon) => icon,
        Err(e) => {
            crate::log_error!("Failed to build Windows tray icon: {:#}", e);
            return None;
        }
    };

    let show_req = show_requested.clone();
    let quit_req = quit_requested.clone();
    let ctx_menu = repaint_ctx.clone();

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            show_req.store(true, Ordering::Relaxed);
            ctx_menu.request_repaint();
        } else if event.id == quit_id {
            quit_req.store(true, Ordering::Relaxed);
            ctx_menu.request_repaint();
        }
    }));

    let show_req_click = show_requested.clone();
    let ctx_click = repaint_ctx;

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                show_req_click.store(true, Ordering::Relaxed);
                ctx_click.request_repaint();
            }
            _ => {}
        }
    }));

    Some(TrayHandle {
        show_requested,
        quit_requested,
        _tray_icon: tray_icon,
    })
}
