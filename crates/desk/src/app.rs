//! The event loop and the widget windows.
use std::time::{Duration, Instant};

use eframe::egui::{self, CentralPanel, Context, ViewportBuilder, ViewportId, WindowLevel};
use inlay_core::config::Layer;
use inlay_core::timefmt::LocalTime;

use crate::clock;
use crate::settings::Settings;

/// How often config.toml is checked for a hand edit.
const RELOAD_EVERY: Duration = Duration::from_secs(2);

/// Starts the event loop and blocks until the app quits.
pub fn run() -> eframe::Result {
    let settings = Settings::load();
    // The root window only hosts the event loop; every widget is its own viewport.
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Inlay")
            .with_inner_size([1.0, 1.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_mouse_passthrough(true)
            .with_window_level(WindowLevel::AlwaysOnBottom),
        ..Default::default()
    };
    eframe::run_native(
        "Inlay",
        options,
        Box::new(|_cc| {
            Ok(Box::new(InlayApp {
                settings,
                reload_at: Instant::now() + RELOAD_EVERY,
            }))
        }),
    )
}

struct InlayApp {
    settings: Settings,
    reload_at: Instant,
}

impl eframe::App for InlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    // Viewports can only be shown from `ui`; `logic` may not show any ui.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        if now >= self.reload_at {
            self.reload_at = now + RELOAD_EVERY;
            self.settings.reload_if_changed();
        }
        // Wake for the next config check even when no widget needs a repaint.
        let mut wake = self.reload_at.saturating_duration_since(now);

        if self.settings.cfg.clock.enabled {
            let time = clock::local_now();
            show_clock(&ctx, &self.settings, &time);
            wake = wake.min(clock::next_change(&self.settings.cfg.clock, &time));
        }
        ctx.request_repaint_after(wake);
    }
}

fn show_clock(ctx: &Context, settings: &Settings, time: &LocalTime) {
    let cfg = &settings.cfg;
    let frame = &cfg.clock.frame;
    // ponytail: 1920x1080 until egui reports the monitor; multi-monitor work areas
    // come with a monitor picker.
    let area = ctx
        .input(|i| i.viewport().monitor_size)
        .unwrap_or(egui::vec2(1920.0, 1080.0));
    let (x, y) = frame.origin(area.x as i32, area.y as i32);
    let level = match cfg.general.layer {
        Layer::Desktop => WindowLevel::AlwaysOnBottom,
        Layer::Normal => WindowLevel::Normal,
        Layer::Top => WindowLevel::AlwaysOnTop,
    };
    let builder = ViewportBuilder::default()
        .with_title("Inlay clock")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_mouse_passthrough(true)
        .with_window_level(level)
        .with_inner_size([frame.w as f32, frame.h as f32])
        // On Wayland a client cannot place its own windows, so the compositor ignores this.
        .with_position([x as f32, y as f32]);
    ctx.show_viewport_immediate(ViewportId::from_hash_of("clock"), builder, |ui, _class| {
        CentralPanel::no_frame().show(ui, |ui| {
            clock::draw(ui.painter(), ui.max_rect(), cfg, time, 1.0);
        });
    });
}
