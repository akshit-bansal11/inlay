//! The app: owns every window and all state, and runs the one loop.
//!
//! The loop drains messages, ticks the widgets that are due, draws the ones
//! that changed, then sleeps for exactly as long as the most impatient widget
//! allows. With nothing playing that is forever: no timer, no frame, no CPU.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use inlay_core::color::{Rgba, parse_hex};
use inlay_core::config::{Config, Layer, StatsStructure};
use inlay_core::lrclib::Query;
use inlay_core::stats::{Sample, Wanted};
use inlay_core::timefmt::LocalTime;
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, HWND, RECT, WAIT_OBJECT_0,
};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{CreateMutexW, INFINITE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_DOWN,
    VK_SHIFT, VK_UP,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, Result, w};

use crate::capture::{self, Audio};
use crate::gfx::{Gfx, Surface};
use crate::lyrics::{self, Lyrics, LyricsState};
use crate::media::{self, Cmd, MediaState};
use crate::menu::{Menu, ORANGE, Pick};
use crate::settings::{Panel, Status};
use crate::sysinfo::{self, Control};
use crate::tray::Tray;
use crate::widgets::{Action, Ctx, Kind, Part, Wake, Widget};
use crate::window::{self, Cover, Event, Mouse, Z};
use crate::{Notify, config_dir, log, now_ms};

const HOTKEY_EDIT: i32 = 1;
const HOTKEY_HIDE: i32 = 2;
const HOTKEY_SETTINGS: i32 = 3;
const HOTKEY_OFFSET_EARLIER: i32 = 4;
const HOTKEY_OFFSET_LATER: i32 = 5;
const HOTKEYS: [(i32, u32); 5] = [
    (HOTKEY_EDIT, b'E' as u32),
    (HOTKEY_HIDE, b'H' as u32),
    (HOTKEY_SETTINGS, b'S' as u32),
    (HOTKEY_OFFSET_EARLIER, VK_UP.0 as u32),
    (HOTKEY_OFFSET_LATER, VK_DOWN.0 as u32),
];

/// Spotify green, used when the accent is automatic and there is no album art.
pub const FALLBACK_ACCENT: Rgba = [0.118, 0.843, 0.376, 1.0];
const WHITE: Rgba = [1.0; 4];
const DEFAULT_CARD: Rgba = [0.063, 0.063, 0.078, 1.0];

const OFFSET_STEP_MS: i32 = 100;
/// Editors save in several writes; wait for them to finish before reloading.
const RELOAD_DELAY_MS: f64 = 200.0;
const SAVE_DELAY_MS: f64 = 500.0;
/// Show Desktop reorders windows a moment after the foreground changes.
const DESKTOP_RECHECK_MS: f64 = 250.0;
/// Space kept between a fitted widget's elements and the edge of its window,
/// which is also the margin of the card drawn behind them.
const FIT_PAD: f32 = 14.0;
const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("Inlay");
const REPO_URL: &str = "https://github.com/akshit-bansal11/inlay";
/// The start-with-Windows entries under the app's earlier names.
const OLD_RUN_NAMES: [PCWSTR; 2] = [w!("Deskbeat"), w!("SonicVeil")];

/// Colours parsed from the config once, not on every frame.
pub struct Palette {
    pub text: Rgba,
    pub card: Rgba,
    /// `None` when the accent follows the album art.
    pub accent: Option<Rgba>,
}

impl Palette {
    pub fn new(cfg: &Config) -> Self {
        Self {
            text: parse_hex(&cfg.theme.text).unwrap_or(WHITE),
            card: parse_hex(&cfg.theme.card_color).unwrap_or(DEFAULT_CARD),
            accent: parse_hex(&cfg.theme.accent),
        }
    }

    pub fn accent(&self, media: &MediaState) -> Rgba {
        self.accent
            .or(media.art.as_ref().and_then(|art| art.accent))
            .unwrap_or(FALLBACK_ACCENT)
    }
}

pub fn local_time() -> (LocalTime, u32) {
    let t = unsafe { GetLocalTime() };
    let time = LocalTime {
        year: u32::from(t.wYear),
        month: u32::from(t.wMonth),
        day: u32::from(t.wDay),
        weekday: u32::from(t.wDayOfWeek),
        hour: u32::from(t.wHour),
        minute: u32::from(t.wMinute),
        second: u32::from(t.wSecond),
    };
    (time, 1000u32.saturating_sub(u32::from(t.wMilliseconds)))
}

/// When a widget should next be ticked.
#[derive(Clone, Copy, PartialEq)]
enum Due {
    Now,
    At(f64),
    Never,
}

struct Host {
    kind: Kind,
    hwnd: HWND,
    surface: Surface,
    widget: Box<dyn Widget>,
    /// Size in physical pixels.
    size: (i32, i32),
    /// Where the widget's box starts inside its window, and the box's size,
    /// in display-independent pixels. The box is the whole window except
    /// for a fitted widget in edit mode, whose window covers the work area.
    origin: (f32, f32),
    extent: (f32, f32),
    visible: bool,
    /// Hidden behind a window that covers its monitor: not ticked, not drawn.
    paused: bool,
    dirty: bool,
    due: Due,
}

struct App {
    gfx: Gfx,
    main: HWND,
    notify: Notify,
    tray: Tray,
    /// The tray icon's menu, while it is open.
    menu: Option<Menu>,
    hosts: Vec<Host>,

    cfg: Config,
    cfg_path: PathBuf,
    /// The config file's text as this app last read or wrote it, so its own
    /// saves are not mistaken for an edit by the user.
    cfg_text: String,
    /// The file on disk does not parse. It is moved aside, not overwritten.
    cfg_broken: bool,
    cfg_watch: Option<HANDLE>,
    reload_at: Option<f64>,
    /// Dragging a slider changes a setting many times a second; the file is
    /// written once the changes stop.
    save_at: Option<f64>,
    hotkeys_on: bool,
    /// Whether the lyrics on screen were fetched with word timing asked for.
    word_sync_on: bool,
    palette: Palette,
    settings: Option<Panel>,

    media: MediaState,
    media_shared: Arc<Mutex<MediaState>>,
    media_tx: Sender<Cmd>,
    lyrics: Lyrics,
    lyrics_gen: u64,
    lyrics_shared: Arc<Mutex<LyricsState>>,
    audio: Arc<Audio>,
    stats: Sample,
    stats_shared: Arc<Mutex<Sample>>,
    sampler: Sender<Control>,
    /// What the sampler thread was last told; `None` is paused.
    sampler_plan: Option<Wanted>,

    /// Pixels per layout pixel for the widgets: the primary monitor's DPI
    /// times how much the layout is grown or shrunk to fit its screen.
    scale: f32,
    /// Pixels per display-independent pixel on the primary monitor, for the
    /// settings panel and the tray menu, which keep their size.
    ui_scale: f32,
    edit: bool,
    hidden: bool,
    /// How much of its monitor the window in front covers, and which monitor.
    cover: Cover,
    cover_monitor: isize,
    /// Show Desktop is active and the widgets were lifted above the desktop.
    raised: bool,
    desktop_recheck_at: Option<f64>,
    device_lost: bool,
    quit: bool,
    /// Where the pointer last was while the selection is being dragged.
    /// Positions are on the screen, not in the window: the window itself
    /// moves as it re-wraps around the element being dragged.
    drag: Option<(f32, f32)>,
    /// The elements chosen in edit mode, which move together.
    selected: Vec<(Kind, u8)>,
    /// The element under the pointer in edit mode, which is outlined.
    hot: Option<(Kind, u8)>,
    /// Whether Windows starts the app at sign-in, as last read from the registry.
    autostart: bool,
}

fn read_config(path: &Path) -> (Config, String, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(cfg) => (cfg, text, false),
            Err(error) => {
                log(&format!(
                    "config.toml does not parse, using defaults: {error}"
                ));
                (Config::default(), text, true)
            }
        },
        Err(_) => (Config::default(), String::new(), false),
    }
}

/// Whether Windows starts the app at sign-in: the Run key holds our value.
fn autostart_enabled() -> bool {
    // SAFETY: both names are NUL-terminated wide literals; no output buffers are passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            RUN_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    status == ERROR_SUCCESS
}

fn set_autostart(on: bool) {
    let done = match (on, std::env::current_exe()) {
        (true, Ok(exe)) => {
            // Quoted, so a path with spaces starts the right program.
            let command: Vec<u16> = format!("\"{}\"", exe.display())
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: `command` is NUL-terminated and outlives the call; the size is in bytes.
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    RUN_SUBKEY,
                    RUN_NAME,
                    REG_SZ.0,
                    Some(command.as_ptr().cast()),
                    (command.len() * 2) as u32,
                )
            };
            status == ERROR_SUCCESS
        }
        (true, Err(_)) => false,
        // SAFETY: both names are NUL-terminated wide literals.
        (false, _) => {
            let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_SUBKEY, RUN_NAME) };
            status == ERROR_SUCCESS
        }
    };
    if !done {
        log("could not change the start-with-Windows setting");
    }
}

pub fn run() -> Result<()> {
    unsafe {
        // A second launch must not exit silently: it asks the first to show itself.
        let _instance = CreateMutexW(None, false, w!("Local\\Inlay.Instance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(existing) = FindWindowW(window::CLASS, window::MAIN_TITLE) {
                window::post(existing, window::WM_APP_SHOW_SETTINGS);
            }
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // An old entry starts an exe that is no longer the app.
    for old in OLD_RUN_NAMES {
        // SAFETY: both names are NUL-terminated wide literals.
        if unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_SUBKEY, old) } == ERROR_SUCCESS {
            set_autostart(true);
        }
    }

    window::register()?;
    let main = window::create_main()?;
    let notify = Notify::new(main);

    // The folder has to exist before it can be watched.
    let _ = std::fs::create_dir_all(config_dir());
    let cfg_path = config_dir().join("config.toml");
    let (cfg, cfg_text, cfg_broken) = read_config(&cfg_path);
    let cfg_watch = unsafe {
        FindFirstChangeNotificationW(
            &HSTRING::from(config_dir().as_os_str()),
            false,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE,
        )
    }
    .ok();

    let media_shared = Arc::new(Mutex::new(MediaState::default()));
    let stats_shared = Arc::default();
    let mut app = App {
        gfx: Gfx::new()?,
        main,
        notify,
        tray: Tray::new(main)?,
        menu: None,
        hosts: Vec::new(),
        palette: Palette::new(&cfg),
        cfg,
        cfg_path,
        cfg_text,
        cfg_broken,
        cfg_watch,
        reload_at: None,
        save_at: None,
        hotkeys_on: false,
        word_sync_on: false,
        settings: None,
        media: MediaState::default(),
        media_tx: media::spawn(media_shared.clone(), notify),
        media_shared,
        lyrics: Lyrics::None,
        lyrics_gen: 0,
        lyrics_shared: Arc::default(),
        audio: capture::spawn(notify)?,
        stats: Sample::default(),
        sampler: sysinfo::spawn(stats_shared.clone(), notify),
        stats_shared,
        sampler_plan: None,
        scale: 1.0,
        ui_scale: 1.0,
        edit: false,
        hidden: false,
        cover: Cover::None,
        cover_monitor: 0,
        raised: false,
        desktop_recheck_at: None,
        device_lost: false,
        quit: false,
        drag: None,
        selected: Vec::new(),
        hot: None,
        autostart: autostart_enabled(),
    };

    for kind in Kind::ALL {
        let hwnd = window::create_widget((0, 0, 100, 100))?;
        app.hosts.push(Host {
            kind,
            hwnd,
            surface: app.gfx.surface(hwnd, 100, 100)?,
            widget: kind.create(),
            size: (100, 100),
            origin: (0.0, 0.0),
            extent: (100.0, 100.0),
            visible: false,
            paused: false,
            dirty: true,
            due: Due::Now,
        });
    }
    let first_run = app.cfg_text.is_empty();
    if first_run {
        // First run: write the defaults out so there is a file to edit.
        app.save_config();
    }
    app.apply_config();
    // A layout from before widgets were re-anchored as they moved: measure
    // each from where it sits now, so it keeps that place on other screens.
    let before = app.cfg.clone();
    app.reanchor(&Kind::ALL);
    if app.cfg != before {
        app.save_at = Some(now_ms() + SAVE_DELAY_MS);
    }
    // Whatever is already in front may be covering the desktop.
    app.foreground_changed(unsafe { GetForegroundWindow() });
    if first_run {
        // First run: the tray icon may be hidden in the overflow, so open settings once to show where everything is.
        app.open_settings();
    }
    app.run_loop();
    if app.save_at.is_some() {
        app.save_config();
    }
    app.tray.remove();
    Ok(())
}

impl App {
    // ----- the loop --------------------------------------------------------

    fn run_loop(&mut self) {
        let mut msg = MSG::default();
        while !self.quit {
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    self.drain_events();
                }
            }
            self.drain_events();
            if self.quit {
                // Checked here, not only at the top: the wait below can be long.
                break;
            }

            match self.frame() {
                // Blocks until the compositor's next refresh.
                Wake::Frame => {
                    if unsafe { DwmFlush() }.is_err() {
                        self.wait(16);
                    }
                }
                Wake::After(delay) => self.wait(delay.as_millis().clamp(1, 3_600_000) as u32),
                Wake::Idle => self.wait(INFINITE),
            }
        }
    }

    fn drain_events(&mut self) {
        while let Some(event) = window::next_event() {
            self.handle(event);
        }
    }

    /// Sleeps until a message arrives, the config folder changes, or `ms` pass.
    fn wait(&mut self, ms: u32) {
        let handles: &[HANDLE] = self.cfg_watch.as_slice();
        let woke = unsafe { MsgWaitForMultipleObjects(Some(handles), false, ms, QS_ALLINPUT) };
        if let Some(watch) = self.cfg_watch
            && woke == WAIT_OBJECT_0
        {
            self.reload_at = Some(now_ms() + RELOAD_DELAY_MS);
            if unsafe { FindNextChangeNotification(watch) }.is_err() {
                self.cfg_watch = None;
            }
        }
    }

    /// Ticks what is due, draws what changed, and says when to run again.
    fn frame(&mut self) -> Wake {
        let now = now_ms();
        if self.reload_at.is_some_and(|at| now >= at) {
            self.reload_at = None;
            self.reload_config();
        }
        if self.desktop_recheck_at.is_some_and(|at| now >= at) {
            self.desktop_recheck_at = None;
            self.check_show_desktop(unsafe { GetForegroundWindow() });
        }
        if self.save_at.is_some_and(|at| now >= at) {
            self.save_at = None;
            self.save_config();
        }
        if self.device_lost {
            self.rebuild_graphics();
        }
        if self.settings.as_ref().is_some_and(|panel| panel.dirty) {
            self.draw_settings();
        }
        if let Some(menu) = self.menu.as_mut().filter(|menu| menu.dirty)
            && let Err(error) = menu.draw(&mut self.gfx, self.ui_scale, self.autostart)
        {
            log(&format!("drawing the tray menu failed: {error}"));
            self.device_lost = true;
        }

        let (time, ms_to_next_second) = local_time();
        let ctx = Ctx {
            cfg: &self.cfg,
            media: &self.media,
            lyrics: &self.lyrics,
            lyrics_gen: self.lyrics_gen,
            audio: &self.audio,
            stats: &self.stats,
            now_ms: now,
            time,
            ms_to_next_second,
            text: self.palette.text,
            card: self.palette.card,
            accent: self.palette.accent(&self.media),
        };

        let mut wake = Wake::Idle;
        // What each fitted widget just drew, to re-wrap its window around.
        let mut fits: Vec<(Kind, Vec<Part>)> = Vec::new();
        for host in &mut self.hosts {
            if !host.visible || host.paused {
                continue;
            }
            if host.due == Due::Now || matches!(host.due, Due::At(at) if now >= at) {
                let tick = host.widget.tick(&ctx);
                host.dirty |= tick.dirty;
                host.due = match tick.wake {
                    Wake::Frame => Due::Now,
                    Wake::After(delay) => Due::At(now + delay.as_secs_f64() * 1000.0),
                    Wake::Idle => Due::Never,
                };
            }
            if host.dirty {
                host.dirty = false;
                // Outlined in edit mode: what is selected, and what the
                // pointer is over.
                let lit: Vec<u8> = self
                    .selected
                    .iter()
                    .chain(&self.hot)
                    .filter(|(kind, _)| self.edit && *kind == host.kind)
                    .map(|(_, id)| *id)
                    .collect();
                if let Err(error) = draw_host(&mut self.gfx, host, &ctx, self.scale, &lit) {
                    log(&format!("drawing {} failed: {error}", host.kind.label()));
                    self.device_lost = true;
                }
                if self.edit {
                    // Tell the window which boxes are elements to drag.
                    let px = |dip: f32| (dip * self.scale).round() as i32;
                    let (ox, oy) = host.origin;
                    let boxes: Vec<RECT> = host
                        .widget
                        .parts()
                        .iter()
                        .map(|(_, r)| RECT {
                            left: px(r.left + ox),
                            top: px(r.top + oy),
                            right: px(r.right + ox),
                            bottom: px(r.bottom + oy),
                        })
                        .collect();
                    window::set_parts(host.hwnd, &boxes, host.kind.fitted());
                }
                if host.kind.fitted() {
                    fits.push((host.kind, host.widget.parts().to_vec()));
                }
            }
            wake = wake.sooner(match host.due {
                Due::Now => Wake::Frame,
                Due::At(at) => Wake::after_ms(at - now),
                Due::Never => Wake::Idle,
            });
        }

        let mut refitted = false;
        for (kind, parts) in fits {
            refitted |= self.fit(kind, &parts);
        }
        if refitted {
            self.sync_windows();
            wake = Wake::Frame;
        }
        if self.settings.as_ref().is_some_and(|panel| panel.dirty) {
            wake = Wake::Frame;
        }
        for deadline in [self.reload_at, self.save_at, self.desktop_recheck_at]
            .into_iter()
            .flatten()
        {
            wake = wake.sooner(Wake::after_ms(deadline - now));
        }
        if self.device_lost {
            wake = wake.sooner(Wake::after_ms(500.0));
        }
        wake
    }

    /// Wraps a fitted widget's window around the elements it just drew, with
    /// a margin, moving the window and shifting the elements by the same
    /// amount so nothing moves on screen. Returns whether anything changed.
    fn fit(&mut self, kind: Kind, parts: &[Part]) -> bool {
        let Some((_, first)) = parts.first() else {
            return false;
        };
        let bounds = parts.iter().fold(*first, |all, (_, r)| D2D_RECT_F {
            left: all.left.min(r.left),
            top: all.top.min(r.top),
            right: all.right.max(r.right),
            bottom: all.bottom.max(r.bottom),
        });
        // Whole pixels, so the window and its contents shift by the same amount.
        let pad = if kind == Kind::Stats {
            self.cfg.stats.padding
        } else {
            FIT_PAD
        };
        let (dx, dy) = ((bounds.left - pad).round(), (bounds.top - pad).round());
        let w = ((bounds.right - bounds.left + 2.0 * pad).ceil() as u32).max(80);
        let h = ((bounds.bottom - bounds.top + 2.0 * pad).ceil() as u32).max(40);

        let frame = *kind.frame_mut(&mut self.cfg);
        if dx == 0.0 && dy == 0.0 && (frame.w, frame.h) == (w, h) {
            return false;
        }
        let (_, _, aw, ah) = window::work_area();
        let area = (
            (aw as f32 / self.scale) as i32,
            (ah as f32 / self.scale) as i32,
        );
        let (left, top) = frame.origin_within(area.0, area.1);
        for id in 0..kind.part_count() {
            kind.nudge(&mut self.cfg, id, -dx, -dy);
        }
        let frame = kind.frame_mut(&mut self.cfg);
        (frame.w, frame.h) = (w, h);
        frame.set_origin(left + dx as i32, top + dy as i32, area.0, area.1);
        for host in self.hosts.iter_mut().filter(|host| host.kind == kind) {
            host.dirty = true;
            host.due = Due::Now;
        }
        true
    }

    /// Makes every widget tick on the next pass: something they read changed.
    fn poke(&mut self) {
        for host in &mut self.hosts {
            host.due = Due::Now;
        }
    }

    fn redraw_all(&mut self) {
        for host in &mut self.hosts {
            host.due = Due::Now;
            host.dirty = true;
        }
    }

    /// Draws the settings panel and acts on whatever was clicked in it.
    fn draw_settings(&mut self) {
        let (_, _, aw, ah) = window::work_area();
        let area = (
            (aw as f32 / self.scale) as i32,
            (ah as f32 / self.scale) as i32,
        );
        let mut cfg = self.cfg.clone();
        let Some(panel) = &mut self.settings else {
            return;
        };
        let status = Status {
            edit: self.edit,
            hidden: self.hidden,
            autostart: self.autostart,
            scale: self.scale,
        };
        let drawn = panel.draw(&mut self.gfx, &mut cfg, self.ui_scale, ORANGE, status, area);
        let outcome = match drawn {
            Ok(outcome) => outcome,
            Err(error) => {
                log(&format!("drawing the settings panel failed: {error}"));
                self.device_lost = true;
                return;
            }
        };
        if outcome.changed {
            self.cfg = cfg.sanitized();
            self.save_at = Some(now_ms() + SAVE_DELAY_MS);
            self.apply_config();
        }
        if outcome.toggle_edit {
            self.toggle_edit();
        }
        if outcome.toggle_hidden {
            self.toggle_hidden();
        }
        if outcome.toggle_autostart {
            self.toggle_autostart();
        }
        if outcome.quit {
            self.quit = true;
        }
        if outcome.open_config {
            self.open_config_file();
        }
    }

    fn toggle_autostart(&mut self) {
        set_autostart(!self.autostart);
        // Read back rather than assumed: the registry write can fail.
        self.autostart = autostart_enabled();
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }
    }

    // ----- tray menu -------------------------------------------------------

    fn open_menu(&mut self) {
        // One that is still open, because it never became the active
        // window, would otherwise be left behind.
        if let Some(menu) = self.menu.take() {
            menu.close();
        }
        match Menu::open(&self.gfx, self.ui_scale) {
            Ok(menu) => self.menu = Some(menu),
            Err(error) => log(&format!("could not open the tray menu: {error}")),
        }
    }

    fn close_menu(&mut self, hwnd: HWND) {
        if let Some(menu) = self.menu.take_if(|menu| menu.hwnd == hwnd) {
            menu.close();
        }
    }

    fn picked(&mut self, pick: Pick) {
        match pick {
            Pick::GitHub => {
                // The shell hands an address to the default browser.
                // SAFETY: the verb is a NUL-terminated literal and the HSTRING outlives the call.
                let opened = unsafe {
                    ShellExecuteW(
                        None,
                        w!("open"),
                        &HSTRING::from(REPO_URL),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                if opened.0 as isize <= 32 {
                    log("could not open the repository page");
                }
            }
            Pick::Autostart => self.toggle_autostart(),
            Pick::Quit => self.quit = true,
        }
    }

    /// After a GPU reset or driver update every device object is dead.
    fn rebuild_graphics(&mut self) {
        // The menu's surface is dead too, and it is cheaper to ask again.
        if let Some(menu) = self.menu.take() {
            menu.close();
        }
        let rebuilt = Gfx::new().and_then(|gfx| {
            for host in &mut self.hosts {
                host.surface = gfx.surface(host.hwnd, host.size.0 as u32, host.size.1 as u32)?;
                host.widget.reset();
            }
            if let Some(panel) = &mut self.settings {
                panel.rebuild(&gfx)?;
            }
            Ok(gfx)
        });
        match rebuilt {
            Ok(gfx) => {
                self.gfx = gfx;
                self.device_lost = false;
                self.redraw_all();
            }
            Err(error) => log(&format!("could not rebuild graphics: {error}")),
        }
    }

    // ----- events ----------------------------------------------------------

    fn handle(&mut self, event: Event) {
        match event {
            Event::Media => self.media_changed(),
            Event::Lyrics => {
                if let Ok(mut shared) = self.lyrics_shared.lock()
                    && shared.track_gen == self.media.track_gen
                {
                    self.lyrics = std::mem::take(&mut shared.lyrics);
                    self.lyrics_gen += 1;
                }
                self.poke();
            }
            Event::Audio => self.poke(),
            Event::Stats => self.stats_changed(),
            Event::Display => {
                self.sync_windows();
                self.redraw_all();
            }
            Event::TaskbarCreated => self.tray.add(),
            Event::ShowSettings => self.open_settings(),
            Event::Tray(WM_LBUTTONDBLCLK) => self.open_settings(),
            Event::Tray(WM_RBUTTONUP) => self.open_menu(),
            Event::Blur(hwnd) => self.close_menu(hwnd),
            Event::Tray(_) => {}
            Event::Hotkey(id) => self.hotkey(id),
            Event::Foreground(hwnd) => self.foreground_changed(hwnd),
            Event::Moved(hwnd) => self.widget_moved(hwnd),
            Event::Mouse { hwnd, kind, x, y } => self.mouse(hwnd, kind, x, y),
            Event::Char { hwnd, code } => {
                if Menu::closes_on(code) {
                    self.close_menu(hwnd);
                }
                if let Some(panel) = self.settings.as_mut().filter(|panel| panel.hwnd == hwnd) {
                    panel.key(code);
                }
            }
            Event::Wheel { hwnd, delta } => {
                if let Some(panel) = self.settings.as_mut().filter(|panel| panel.hwnd == hwnd) {
                    panel.wheel(delta);
                }
            }
            Event::Close(hwnd) => {
                self.close_menu(hwnd);
                if let Some(panel) = self.settings.take_if(|panel| panel.hwnd == hwnd) {
                    panel.close();
                }
            }
        }
    }

    fn media_changed(&mut self) {
        let Ok(new) = self.media_shared.lock().map(|state| state.clone()) else {
            return;
        };
        let was_present = self.media.present;
        let new_track = new.track_gen != self.media.track_gen;
        self.media = new;

        if self.media.present != was_present {
            // Spotify opened or closed: the capture thread has a process to
            // find or lose, and the Spotify-only widgets appear or go.
            self.audio.kick();
            self.sync_windows();
        }
        if new_track || (self.media.present && matches!(self.lyrics, Lyrics::None)) {
            self.audio.kick();
            self.fetch_lyrics();
        }
        self.poke();
    }

    fn fetch_lyrics(&mut self) {
        let wanted = self.cfg.lyrics.enabled && self.media.present && !self.media.title.is_empty();
        self.lyrics = if wanted {
            Lyrics::Loading
        } else {
            Lyrics::None
        };
        self.lyrics_gen += 1;
        self.word_sync_on = self.cfg.lyrics.word_sync;
        if let Ok(mut shared) = self.lyrics_shared.lock() {
            shared.track_gen = self.media.track_gen;
            shared.lyrics = Lyrics::None;
        }
        if wanted {
            let query = Query {
                title: self.media.title.clone(),
                artist: self.media.artist.clone(),
                album: self.media.album.clone(),
                duration_ms: self.media.duration_ms as i64,
            };
            lyrics::fetch(
                query,
                self.media.track_gen,
                self.lyrics_shared.clone(),
                self.notify,
                self.word_sync_on
                    .then(|| self.cfg.lyrics.word_servers.clone()),
            );
        }
    }

    fn host_index(&self, hwnd: HWND) -> Option<usize> {
        self.hosts.iter().position(|host| host.hwnd == hwnd)
    }

    fn mouse(&mut self, hwnd: HWND, kind: Mouse, x: i32, y: i32) {
        if let Some(panel) = self.settings.as_mut().filter(|panel| panel.hwnd == hwnd) {
            panel.mouse(kind, x as f32 / self.ui_scale, y as f32 / self.ui_scale);
            return;
        }
        if let Some(menu) = self.menu.as_mut().filter(|menu| menu.hwnd == hwnd) {
            let pick = menu.mouse(kind, x as f32 / self.ui_scale, y as f32 / self.ui_scale);
            if let Some(pick) = pick {
                self.close_menu(hwnd);
                self.picked(pick);
            }
            return;
        }
        let Some(index) = self.host_index(hwnd) else {
            return;
        };
        if self.edit {
            let (left, top, _, _) = window::bounds(hwnd);
            let dip = |px: i32| px as f32 / self.scale;
            let (ox, oy) = self.hosts[index].origin;
            let at = (dip(x) - ox, dip(y) - oy);
            self.drag_part(index, kind, at, (dip(left + x), dip(top + y)));
            return;
        }
        let host = &mut self.hosts[index];
        let at = (x as f32 / self.scale, y as f32 / self.scale);
        let action = match kind {
            Mouse::Move => {
                host.dirty |= host.widget.hover(Some(at));
                None
            }
            Mouse::Leave => {
                host.dirty |= host.widget.hover(None);
                None
            }
            Mouse::Up => host.widget.click(at.0, at.1),
            Mouse::Down => None,
        };
        // A send fails only if the media thread is gone; there is nothing to control then.
        let _ = match action {
            Some(Action::PlayPause) => self.media_tx.send(Cmd::PlayPause),
            Some(Action::Next) => self.media_tx.send(Cmd::Next),
            Some(Action::Previous) => self.media_tx.send(Cmd::Previous),
            Some(Action::Seek(fraction)) => self
                .media_tx
                .send(Cmd::SeekMs(fraction * self.media.duration_ms)),
            None => Ok(()),
        };
    }

    /// Edit mode: moves one element of a widget with the pointer.
    ///
    /// `at` is the pointer inside the widget and `screen` the same point on
    /// the screen, both in display-independent pixels.
    fn drag_part(&mut self, index: usize, kind: Mouse, at: (f32, f32), screen: (f32, f32)) {
        let ((x, y), (sx, sy)) = (at, screen);
        match kind {
            Mouse::Down => {
                let Some(id) = self.part_under(index, x, y) else {
                    return;
                };
                let part = (self.hosts[index].kind, id);
                let shift = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                if shift {
                    // Shift adds an element to the selection, or takes out
                    // one that is already in it.
                    match self.selected.iter().position(|chosen| *chosen == part) {
                        Some(at) => {
                            self.selected.remove(at);
                        }
                        None => self.selected.push(part),
                    }
                } else if !self.selected.contains(&part) {
                    // A plain click on something outside the selection
                    // starts a new one. On something inside it, the whole
                    // selection is about to be dragged.
                    self.selected = vec![part];
                }
                self.drag = self.selected.contains(&part).then_some((sx, sy));
                self.redraw_all();
            }
            Mouse::Move => {
                let Some((last_x, last_y)) = self.drag else {
                    // Not dragging: outline whatever the pointer is over.
                    let over = self.part_under(index, x, y);
                    self.set_hot(index, over);
                    return;
                };
                // Dragging a System reading takes it out of the strip, list or
                // grid: each reading is pinned where it is now, then moves.
                if self.cfg.stats.structure != StatsStructure::Free
                    && self.selected.iter().any(|(kind, _)| *kind == Kind::Stats)
                {
                    let spots: Vec<(u8, f32, f32)> = self
                        .hosts
                        .iter()
                        .filter(|host| host.kind == Kind::Stats)
                        .flat_map(|host| host.widget.parts())
                        .map(|(id, r)| (*id, r.left, r.top))
                        .collect();
                    self.cfg.stats.seed_free(&spots);
                    for host in self
                        .hosts
                        .iter_mut()
                        .filter(|host| host.kind == Kind::Stats)
                    {
                        host.dirty = true;
                    }
                }
                // Everything selected moves together, across widgets too.
                for &(kind, id) in &self.selected {
                    kind.nudge(&mut self.cfg, id, sx - last_x, sy - last_y);
                }
                self.drag = Some((sx, sy));
                for host in &mut self.hosts {
                    host.dirty |= self.selected.iter().any(|(kind, _)| *kind == host.kind);
                }
                // Its position sliders follow.
                if let Some(panel) = &mut self.settings {
                    panel.dirty = true;
                }
            }
            Mouse::Up => {
                if self.drag.take().is_some() {
                    let kinds: Vec<Kind> = self.selected.iter().map(|(kind, _)| *kind).collect();
                    self.reanchor(&kinds);
                    self.cfg = self.cfg.clone().sanitized();
                    self.save_config();
                    if let Some(panel) = &mut self.settings {
                        panel.dirty = true;
                    }
                }
            }
            Mouse::Leave => {
                if self.drag.is_none() {
                    self.set_hot(index, None);
                }
            }
        }
    }

    /// The element of a widget at a point inside it. The last one drawn is
    /// on top, so it wins where two overlap.
    fn part_under(&self, index: usize, x: f32, y: f32) -> Option<u8> {
        self.hosts[index]
            .widget
            .parts()
            .iter()
            .rev()
            .find(|(_, r)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
            .map(|(id, _)| *id)
    }

    /// Records which element of a widget the pointer is over, redrawing the
    /// widget when that changes.
    fn set_hot(&mut self, index: usize, part: Option<u8>) {
        let kind = self.hosts[index].kind;
        let hot = part.map(|id| (kind, id));
        if hot != self.hot {
            self.hot = hot;
            self.hosts[index].dirty = true;
        }
    }

    /// Measures each widget's frame from the edge, corner or centre it is
    /// nearest, so it keeps that place on a screen of another size.
    fn reanchor(&mut self, kinds: &[Kind]) {
        let (_, _, aw, ah) = window::work_area();
        let area = (
            (aw as f32 / self.scale) as i32,
            (ah as f32 / self.scale) as i32,
        );
        for &kind in kinds {
            kind.frame_mut(&mut self.cfg).reanchor(area.0, area.1);
        }
    }

    fn widget_moved(&mut self, hwnd: HWND) {
        let Some(index) = self.host_index(hwnd) else {
            return;
        };
        let kind = self.hosts[index].kind;
        let (x, y, w, h) = window::bounds(hwnd);
        let (ax, ay, aw, ah) = window::work_area();
        let dip = |px: i32| (px as f32 / self.scale).round() as i32;

        let spanning = kind == Kind::Visualizer && self.cfg.visualizer.full_width;
        if spanning && dip(w) == dip(aw) && dip(x - ax) == 0 {
            // Still edge to edge: only its height and vertical place changed.
            let mut placed = self.cfg.visualizer.placed(dip(aw));
            placed.h = dip(h).max(1) as u32;
            placed.set_origin(0, dip(y - ay), dip(aw), dip(ah));
            let frame = &mut self.cfg.visualizer.frame;
            (frame.y, frame.h) = (placed.y, placed.h);
        } else {
            // Dragged off the edges: it is an ordinary box from here on.
            self.cfg.visualizer.full_width &= kind != Kind::Visualizer;
            let frame = kind.frame_mut(&mut self.cfg);
            frame.w = dip(w).max(1) as u32;
            frame.h = dip(h).max(1) as u32;
            frame.set_origin(dip(x - ax), dip(y - ay), dip(aw), dip(ah));
        }
        kind.frame_mut(&mut self.cfg).reanchor(dip(aw), dip(ah));
        self.cfg = self.cfg.clone().sanitized();
        self.save_config();
        self.sync_windows();
        // The panel's position and size sliders show where it was a moment ago.
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }
    }

    fn foreground_changed(&mut self, hwnd: HWND) {
        let own_panel = self
            .settings
            .as_ref()
            .is_some_and(|panel| panel.hwnd == hwnd);
        if hwnd == self.main || own_panel || self.host_index(hwnd).is_some() {
            return;
        }
        let front = (window::cover(hwnd), window::monitor(hwnd));
        if front != (self.cover, self.cover_monitor) {
            (self.cover, self.cover_monitor) = front;
            self.sync_windows();
            self.redraw_all();
        }
        self.check_show_desktop(hwnd);
        self.desktop_recheck_at = Some(now_ms() + DESKTOP_RECHECK_MS);
    }

    /// Keeps desktop-layer widgets visible through Show Desktop (Win+D), which
    /// raises the desktop over every window, bottom-pinned ones included.
    fn check_show_desktop(&mut self, foreground: HWND) {
        if self.cfg.general.layer != Layer::Desktop || self.edit {
            return;
        }
        let Some(probe) = self.hosts.iter().find(|host| host.visible) else {
            return;
        };
        let covered = window::desktop_covers(probe.hwnd);
        if covered && !self.raised {
            self.raised = true;
            window::set_pin_bottom(false);
            for host in &self.hosts {
                // Topmost then not: lands at the top of the ordinary windows,
                // which is just above the raised desktop.
                window::set_z(host.hwnd, Z::Top);
                window::set_z(host.hwnd, Z::Normal);
            }
        } else if self.raised && !covered && !window::is_shell(foreground) {
            // A real window is in front again: back under everything.
            self.raised = false;
            self.sync_windows();
        }
    }

    // ----- commands --------------------------------------------------------

    fn hotkey(&mut self, id: i32) {
        match id {
            HOTKEY_EDIT => self.toggle_edit(),
            HOTKEY_HIDE => self.toggle_hidden(),
            HOTKEY_SETTINGS => self.open_settings(),
            HOTKEY_OFFSET_EARLIER => self.nudge_offset(OFFSET_STEP_MS),
            HOTKEY_OFFSET_LATER => self.nudge_offset(-OFFSET_STEP_MS),
            _ => {}
        }
    }

    fn toggle_edit(&mut self) {
        self.edit = !self.edit;
        self.raised = false;
        self.drag = None;
        self.selected.clear();
        self.hot = None;
        if !self.edit {
            for host in &self.hosts {
                window::set_parts(host.hwnd, &[], false);
            }
        }
        if self.edit {
            self.hidden = false;
        }
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }
        self.sync_windows();
        self.redraw_all();
    }

    fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
        self.sync_windows();
    }

    fn nudge_offset(&mut self, by_ms: i32) {
        self.cfg.lyrics.offset_ms = (self.cfg.lyrics.offset_ms + by_ms).clamp(-10_000, 10_000);
        self.save_config();
        self.poke();
    }

    fn open_settings(&mut self) {
        if let Some(panel) = &self.settings {
            panel.focus();
            return;
        }
        match Panel::open(&self.gfx, self.ui_scale) {
            Ok(panel) => self.settings = Some(panel),
            Err(error) => {
                // The panel is a convenience; the file is always editable.
                log(&format!("could not open the settings panel: {error}"));
                self.open_config_file();
            }
        }
    }

    fn open_config_file(&mut self) {
        if !self.cfg_path.exists() {
            self.save_config();
        }
        // A full path: a bare name would be looked up beside the exe first.
        let notepad = std::env::var_os("WINDIR")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join("System32")
            .join("notepad.exe");
        if let Err(error) = Command::new(notepad).arg(&self.cfg_path).spawn() {
            log(&format!("could not open the config file: {error}"));
        }
    }

    // ----- config ----------------------------------------------------------

    fn save_config(&mut self) {
        self.save_at = None;
        let text = self.cfg.to_toml();
        let written = std::fs::create_dir_all(config_dir()).and_then(|()| {
            if self.cfg_broken {
                // Never overwrite a file the user wrote and this app could not read.
                std::fs::rename(&self.cfg_path, self.cfg_path.with_extension("broken.toml"))?;
            }
            std::fs::write(&self.cfg_path, &text)
        });
        match written {
            Ok(()) => {
                self.cfg_text = text;
                self.cfg_broken = false;
            }
            Err(error) => log(&format!("could not save config.toml: {error}")),
        }
    }

    /// The config folder changed on disk: pick up an edit made by the user.
    fn reload_config(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.cfg_path) else {
            return;
        };
        if text == self.cfg_text {
            return;
        }
        match Config::from_toml(&text) {
            Ok(cfg) => {
                self.cfg = cfg;
                self.cfg_broken = false;
                self.apply_config();
            }
            Err(error) => {
                // Keep running on the last good config while the file is mid-edit.
                log(&format!(
                    "config.toml does not parse, keeping the old settings: {error}"
                ));
                self.cfg_broken = true;
            }
        }
        self.cfg_text = text;
    }

    /// Pushes `self.cfg` out to everything that depends on it.
    fn apply_config(&mut self) {
        self.palette = Palette::new(&self.cfg);
        self.audio.set_source(
            self.cfg
                .visualizer
                .enabled
                .then_some(self.cfg.visualizer.source),
        );

        if self.cfg.general.hotkeys != self.hotkeys_on {
            self.hotkeys_on = self.cfg.general.hotkeys;
            let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
            for (id, key) in HOTKEYS {
                let changed = unsafe {
                    if self.hotkeys_on {
                        RegisterHotKey(Some(self.main), id, modifiers, key)
                    } else {
                        UnregisterHotKey(Some(self.main), id)
                    }
                };
                if changed.is_err() && self.hotkeys_on {
                    log(&format!("hotkey {id} is taken by another app"));
                }
            }
        }
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }

        if self.cfg.lyrics.enabled == matches!(self.lyrics, Lyrics::None)
            || self.cfg.lyrics.word_sync != self.word_sync_on
        {
            // Lyrics were just switched on with a track playing, or off, or
            // are now wanted from a different source.
            self.fetch_lyrics();
        }
        for host in &mut self.hosts {
            host.widget.reset();
        }
        self.sync_windows();
        self.redraw_all();
    }

    /// Positions, sizes, shows, hides and orders every widget window.
    fn sync_windows(&mut self) {
        let (ax, ay, aw, ah) = window::work_area();
        self.ui_scale = window::primary_dpi() as f32 / 96.0;
        let area_h = ah as f32 / self.ui_scale;
        if self.cfg.general.layout_height == 0 {
            // The first screen seen is the one the layout is measured on.
            self.cfg.general.layout_height = area_h.round() as u32;
            self.save_at = Some(now_ms() + SAVE_DELAY_MS);
        }
        self.scale = self.ui_scale * self.cfg.general.layout_scale(area_h);
        let scale = self.scale;
        let px = |dip: f32| (dip * scale).round() as i32;
        let layer = self.cfg.general.layer;
        let pausing =
            self.cfg.general.pause_on_fullscreen && !self.edit && self.cover != Cover::None;

        // Unpinned while reordering, or the pin would undo it.
        window::set_pin_bottom(false);
        window::set_edit_mode(self.edit);

        for host in &mut self.hosts {
            let frame = host.kind.placed(&self.cfg, (aw as f32 / scale) as i32);
            let (left, top) =
                frame.origin_within((aw as f32 / scale) as i32, (ah as f32 / scale) as i32);
            // In edit mode a fitted widget's window covers the work area and
            // the widget is drawn at its place inside it. Re-wrapping the
            // window while an element is dragged moved it a frame before the
            // elements were redrawn, which made the other elements jump.
            let spread = self.edit && host.kind.fitted();
            let bounds = if spread {
                (ax, ay, aw.max(1), ah.max(1))
            } else {
                (
                    ax + px(left as f32),
                    ay + px(top as f32),
                    px(frame.w as f32).max(1),
                    px(frame.h as f32).max(1),
                )
            };
            let (origin, extent) = if spread {
                ((left as f32, top as f32), (frame.w as f32, frame.h as f32))
            } else {
                (
                    (0.0, 0.0),
                    (bounds.2 as f32 / scale, bounds.3 as f32 / scale),
                )
            };
            host.dirty |= (host.origin, host.extent) != (origin, extent);
            (host.origin, host.extent) = (origin, extent);
            if window::bounds(host.hwnd) != bounds {
                window::set_bounds(host.hwnd, bounds);
            }
            if host.size != (bounds.2, bounds.3) {
                host.size = (bounds.2, bounds.3);
                if let Err(error) =
                    self.gfx
                        .resize(&mut host.surface, bounds.2 as u32, bounds.3 as u32)
                {
                    log(&format!("resizing {} failed: {error}", host.kind.label()));
                    self.device_lost = true;
                }
                host.dirty = true;
            }

            // A widget under a window that covers its monitor cannot be seen,
            // so it stops running. An always-on-top widget is above a maximized
            // window and keeps going; under true fullscreen it is hidden.
            host.paused = pausing
                && window::monitor(host.hwnd) == self.cover_monitor
                && (layer != Layer::Top || self.cover == Cover::Fullscreen);

            let has_content = !host.kind.needs_spotify()
                || self.media.present
                || !self.cfg.general.hide_without_spotify;
            let visible = host.kind.enabled(&self.cfg)
                && !self.hidden
                && !(host.paused && layer == Layer::Top)
                && (self.edit || has_content);
            if visible != host.visible {
                host.visible = visible;
                window::show(host.hwnd, visible);
                host.dirty = true;
                host.due = Due::Now;
            }

            window::set_click_through(host.hwnd, !self.edit && !host.kind.interactive());
            if self.edit || layer == Layer::Top {
                window::set_z(host.hwnd, Z::Top);
            } else if layer == Layer::Normal {
                window::set_z(host.hwnd, Z::Normal);
            } else if !self.raised {
                window::set_z(host.hwnd, Z::Normal);
                window::set_z(host.hwnd, Z::Bottom);
            }
        }

        window::set_pin_bottom(layer == Layer::Desktop && !self.edit && !self.raised);
        self.sync_sampler();
    }

    /// Tells the sampler thread what to read, or to stop while no System
    /// widget is on screen: nothing is read, and nothing wakes the app.
    fn sync_sampler(&mut self) {
        let on = self
            .hosts
            .iter()
            .any(|h| h.kind == Kind::Stats && h.visible && !h.paused);
        let plan = on.then(|| self.cfg.stats.wanted());
        if plan == self.sampler_plan {
            return;
        }
        // A send fails only when the thread is gone, as with the media thread.
        let _ = self.sampler.send(match &plan {
            Some(wanted) => Control::Run(wanted.clone()),
            None => Control::Pause,
        });
        if plan.is_none() {
            self.stats = Sample::default();
        }
        self.sampler_plan = plan;
    }

    /// A new reading arrived from the sampler thread.
    fn stats_changed(&mut self) {
        let Ok(new) = self.stats_shared.lock().map(|sample| sample.clone()) else {
            return;
        };
        self.stats = new;
        for host in self.hosts.iter_mut().filter(|h| h.kind == Kind::Stats) {
            host.due = Due::Now;
        }
    }
}

/// Draws one widget into its window. `lit` are the elements to outline in
/// edit mode: the selected ones and the one under the pointer. Nothing else
/// marks a widget's bounds.
fn draw_host(gfx: &mut Gfx, host: &mut Host, ctx: &Ctx, scale: f32, lit: &[u8]) -> Result<()> {
    let ((x, y), (w, h)) = (host.origin, host.extent);
    gfx.begin(&host.surface);
    gfx.set_transform(scale, x * scale, y * scale);
    let drawn = host.widget.draw(gfx, w, h, ctx);
    for (id, r) in host.widget.parts() {
        if lit.contains(id) {
            gfx.stroke_round(*r, 3.0, ctx.accent, 1.5);
        }
    }
    gfx.end(&host.surface).and(drawn)
}
