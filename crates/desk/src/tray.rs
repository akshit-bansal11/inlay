//! The menu-bar / system-tray icon: the only way to quit and to reach the config.
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};

use inlay_core::mark;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::paths;

/// Side of the icon in pixels.
const ICON_SIDE: u32 = 32;

pub enum TrayAction {
    OpenConfig,
    OpenFonts,
    Quit,
}

pub struct Tray {
    /// Dropping it removes the icon.
    _icon: TrayIcon,
    actions: Receiver<TrayAction>,
}

impl Tray {
    /// Builds the icon and its menu; `wake` is called when an item is chosen so the app
    /// repaints and reads it. On macOS call this on the main thread once the event loop
    /// runs. `None` when there is no tray (logged); then the only way to quit is the
    /// system's (Activity Monitor, `pkill inlay`).
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let open_config = MenuItem::new("Open config file", true, None);
        let open_fonts = MenuItem::new("Open fonts folder", true, None);
        let quit = MenuItem::new("Quit Inlay", true, None);
        let menu = Menu::new();
        let appended = menu
            .append(&open_config)
            .and(menu.append(&open_fonts))
            .and(menu.append(&PredefinedMenuItem::separator()))
            .and(menu.append(&quit));
        if let Err(error) = appended {
            eprintln!("tray: menu not built: {error}");
            return None;
        }

        let (open_config, open_fonts, quit) = (
            open_config.id().clone(),
            open_fonts.id().clone(),
            quit.id().clone(),
        );
        let (sender, actions) = channel();
        // The handler replaces MenuEvent::receiver, so the events are turned into
        // actions here and the app is woken to read them.
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = if event.id == open_config {
                TrayAction::OpenConfig
            } else if event.id == open_fonts {
                TrayAction::OpenFonts
            } else if event.id == quit {
                TrayAction::Quit
            } else {
                return;
            };
            if sender.send(action).is_ok() {
                wake();
            }
        }));

        let icon = match icon() {
            Ok(icon) => icon,
            Err(error) => {
                eprintln!("tray: icon not built: {error}");
                return None;
            }
        };
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Inlay")
            .build()
        {
            Ok(icon) => Some(Self {
                _icon: icon,
                actions,
            }),
            Err(error) => {
                eprintln!("tray: no tray icon ({error}); quit with the system's tools");
                None
            }
        }
    }

    /// The next menu choice, if one was made.
    pub fn poll(&self) -> Option<TrayAction> {
        self.actions.try_recv().ok()
    }
}

/// The Inlay mark as a tray icon.
fn icon() -> Result<Icon, tray_icon::BadIcon> {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut rgba = Vec::with_capacity((ICON_SIDE * ICON_SIDE * 4) as usize);
    for y in 0..ICON_SIDE {
        for x in 0..ICON_SIDE {
            let pixel = mark::pixel(x as f32 + 0.5, y as f32 + 0.5, ICON_SIDE as f32);
            rgba.extend(pixel.map(byte));
        }
    }
    Icon::from_rgba(rgba, ICON_SIDE, ICON_SIDE)
}

/// Opens a file or folder with the system's default app.
pub fn open(path: &Path) {
    #[cfg(target_os = "macos")]
    let opener = "/usr/bin/open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    if let Err(error) = std::process::Command::new(opener).arg(path).spawn() {
        eprintln!("tray: {opener} {} failed: {error}", path.display());
    }
}

/// Opens config.toml.
pub fn open_config() {
    if let Some(path) = paths::config_file() {
        open(&path);
    }
}

/// Opens the fonts folder, creating it first.
pub fn open_fonts() {
    if let Some(dir) = paths::fonts_dir() {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            eprintln!("tray: cannot create {}: {error}", dir.display());
            return;
        }
        open(&dir);
    }
}
