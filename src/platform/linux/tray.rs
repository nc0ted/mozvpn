use eframe::egui;
use ksni::TrayMethods;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct TrayHandle {
    pub show_requested: Arc<AtomicBool>,
    pub quit_requested: Arc<AtomicBool>,
}

struct MozTray {
    show_requested: Arc<AtomicBool>,
    quit_requested: Arc<AtomicBool>,
    repaint_ctx: Option<egui::Context>,
}

impl ksni::Tray for MozTray {
    fn id(&self) -> String {
        "mozvpn".into()
    }

    fn title(&self) -> String {
        "MozVPN".into()
    }

    fn icon_name(&self) -> String {
        "mozvpn".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        if let Ok(img) = image::load_from_memory(include_bytes!("../../../assets/icon.png")) {
            let (width, height) = (img.width(), img.height());
            let mut data = img.into_rgba8().into_vec();
            for pixel in data.as_chunks_mut::<4>().0 {
                pixel.rotate_right(1);
            }
            vec![ksni::Icon {
                width: width as i32,
                height: height as i32,
                data,
            }]
        } else {
            Vec::new()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.show_requested.store(true, Ordering::Relaxed);
        if let Some(ctx) = &self.repaint_ctx {
            ctx.request_repaint();
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let show_req = self.show_requested.clone();
        let quit_req = self.quit_requested.clone();
        let ctx_show = self.repaint_ctx.clone();
        let ctx_quit = self.repaint_ctx.clone();

        vec![
            StandardItem {
                label: "Show MozVPN".into(),
                activate: Box::new(move |_| {
                    show_req.store(true, Ordering::Relaxed);
                    if let Some(ctx) = &ctx_show {
                        ctx.request_repaint();
                    }
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_| {
                    quit_req.store(true, Ordering::Relaxed);
                    if let Some(ctx) = &ctx_quit {
                        ctx.request_repaint();
                    }
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn_tray(rt: &tokio::runtime::Runtime, repaint_ctx: egui::Context) -> Option<TrayHandle> {
    let show_requested = Arc::new(AtomicBool::new(false));
    let quit_requested = Arc::new(AtomicBool::new(false));

    let tray = MozTray {
        show_requested: show_requested.clone(),
        quit_requested: quit_requested.clone(),
        repaint_ctx: Some(repaint_ctx),
    };

    rt.spawn(async move {
        let _ = tray.spawn().await;
    });

    Some(TrayHandle {
        show_requested,
        quit_requested,
    })
}
