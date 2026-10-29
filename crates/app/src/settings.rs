//! The settings panel: a small immediate-mode UI drawn with the same
//! Direct2D context as the widgets. Each control reads and writes one config
//! field in place, so there is no second copy of the settings to keep in step.
//!
//! A sidebar picks a widget, a row of sections under the page's title picks
//! one part of it, and the rows of that part scroll beneath in cards. Every
//! setting has one row, on one page.
//!
//! Free-form values (any installed font, a hand-written clock format) are
//! not editable here; the config file takes those.

use std::ffi::c_void;

use inlay_core::color::{Rgba, hsv_to_rgb, parse_hex, rgb_to_hsv, to_hex, with_alpha};
use inlay_core::config::*;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DestroyWindow, GetClientRect, SW_SHOWNORMAL, SetForegroundWindow,
    ShowWindow, WINDOW_EX_STYLE, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::{BOOL, Result, w};

use crate::gfx::{Gfx, ICON_FONT, Surface, TextStyle, rect};
use crate::menu::{BONE, INK, mark};
use crate::window::{self, Mouse};

pub const WIDTH: f32 = 780.0;
pub const HEIGHT: f32 = 660.0;
const SIDEBAR: f32 = 196.0;
const MARGIN: f32 = 24.0;
/// The page's title and its row of sections, above the scrolling rows.
const HEADER: f32 = 108.0;
const ROW: f32 = 40.0;
const COLOR_ROW: f32 = 66.0;
/// A card's corner radius, which is also the space above its first row and
/// below its last: a row's square corners must stay clear of the round ones.
const CARD_RADIUS: f32 = 10.0;
/// How far a card's rows sit in from its sides.
const CARD_INSET: f32 = 16.0;
/// How near a quarter mark, along a position slider, the knob jumps onto it.
const SNAP_REACH: f32 = 7.0;
/// The picker's saturation and brightness square, and the hue bar under it.
const PICKER_SQUARE: (f32, f32) = (210.0, 120.0);
const HUE_BAR_HEIGHT: f32 = 16.0;
/// The six primaries a hue bar passes through, in order.
const HUE_STOPS: [Rgba; 7] = [
    [1.0, 0.0, 0.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [1.0, 0.0, 1.0, 1.0],
    [1.0, 0.0, 0.0, 1.0],
];
/// Share of a row's width given to the control rather than the label.
const CONTROL_SHARE: f32 = 0.58;
const WHEEL_STEP: f32 = 56.0;

const BACKGROUND: Rgba = INK;
const TEXT: Rgba = [BONE[0], BONE[1], BONE[2], 0.92];
const DIM: Rgba = [BONE[0], BONE[1], BONE[2], 0.56];
const TRACK: Rgba = [1.0, 1.0, 1.0, 0.14];
/// Opaque, because a card is painted in overlapping pieces.
const CARD: Rgba = [0.11, 0.11, 0.125, 1.0];
const SIDE: Rgba = [0.055, 0.055, 0.065, 1.0];
const HAIRLINE: Rgba = [BONE[0], BONE[1], BONE[2], 0.07];

pub const TABS: [&str; 5] = ["General", "Clock", "Player", "Lyrics", "Visualizer"];
/// Segoe Fluent Icons for the sidebar: settings, clock, music, text, chart.
const TAB_ICONS: [&str; 5] = ["\u{E713}", "\u{E823}", "\u{E8D6}", "\u{E8D2}", "\u{E9D9}"];
/// The parts of each tab's page. Each is one screenful, more or less.
const SECTIONS: [&[&str]; 5] = [
    &["Appearance", "Behaviour", "App"],
    &["Widget", "Day", "Time", "Date"],
    &[
        "Widget", "Art", "Title", "Artist", "Buttons", "Bar", "Played", "Length",
    ],
    &["Widget", "Text", "Colours", "Timing"],
    &["Widget", "Bars", "Colour", "Motion"],
];
const SWATCHES: [&str; 10] = [
    "#FFFFFF", "#1ED760", "#7C5CFF", "#21D4FD", "#FF3DCB", "#FF6B6B", "#FFB547", "#F9F871",
    "#101014", "#000000",
];
/// Fonts every Windows 11 has, offered for any piece of text. Fonts in the
/// app's own folder are listed ahead of these; anything else can be named
/// in the config file.
const SYSTEM_FONTS: [&str; 12] = [
    "Segoe UI Variable Display",
    "Segoe UI",
    "Bahnschrift",
    "Cascadia Code",
    "Consolas",
    "Georgia",
    "Arial",
    "Calibri",
    "Verdana",
    "Trebuchet MS",
    "Times New Roman",
    "Impact",
];
const FONTS: [(&str, &str); 7] = [
    ("Segoe UI Variable Display", "Segoe UI Variable"),
    ("Segoe UI", "Segoe UI"),
    ("Bahnschrift", "Bahnschrift"),
    ("Cascadia Code", "Cascadia Code"),
    ("Georgia", "Georgia"),
    ("Consolas", "Consolas"),
    ("Arial", "Arial"),
];
const TIME_FORMATS: [(&str, &str); 4] = [
    ("%H:%M", "21:47"),
    ("%H:%M:%S", "21:47:09"),
    ("%l:%M %p", "9:47 PM"),
    ("%l:%M", "9:47"),
];
const DAY_FORMATS: [(&str, &str); 2] = [("%A", "Saturday"), ("%a", "Sat")];
const DATE_FORMATS: [(&str, &str); 4] = [
    ("%e %B %Y", "3 October 2026"),
    ("%B %e, %Y", "October 3, 2026"),
    ("%d/%m/%Y", "03/10/2026"),
    ("%Y-%m-%d", "2026-10-03"),
];
/// Colours a lyric setting can follow by name, ahead of the swatches.
const THEME_COLORS: [(&str, &str); 2] = [("text", "Text"), ("accent", "Accent")];
const ALIGNS: [(Align, &str); 3] = [
    (Align::Left, "Left"),
    (Align::Center, "Centre"),
    (Align::Right, "Right"),
];

/// The same choice for a line that runs down or up: it grows along its length.
const RUNNING_ALIGNS: [(Align, &str); 3] = [
    (Align::Left, "Top left"),
    (Align::Center, "Middle"),
    (Align::Right, "Bottom right"),
];

const DIRECTIONS: [(Direction, &str); 3] = [
    (Direction::Horizontal, "Across"),
    (Direction::Down, "Down"),
    (Direction::Up, "Up"),
];
const LETTERS: [(Letters, &str); 2] = [
    (Letters::Upright, "Upright"),
    (Letters::Sideways, "Sideways"),
];

/// What the panel needs to know about the app to draw itself.
#[derive(Clone, Copy)]
pub struct Status {
    pub edit: bool,
    pub hidden: bool,
    pub autostart: bool,
    /// Physical pixels per display-independent pixel, for showing positions
    /// in the display's own pixels.
    pub scale: f32,
}

/// What the user did in the panel that the app has to act on.
#[derive(Default)]
pub struct Outcome {
    /// A setting changed and `cfg` holds the new value.
    pub changed: bool,
    pub toggle_edit: bool,
    pub toggle_hidden: bool,
    pub toggle_autostart: bool,
    pub quit: bool,
    pub open_config: bool,
    /// This pass changed something the next pass has to show.
    pub redraw: bool,
}

/// The panel's contents and pointer state, apart from any window, so the
/// same code paints the real panel and the offscreen snapshot of it.
/// The colour picker, which belongs to at most one colour row at a time.
#[derive(Default)]
struct Picker {
    /// The row it is open under.
    open: Option<u32>,
    /// Hue in degrees, saturation, value. Kept here rather than read back
    /// from the colour: grey and black have no hue, so reading it back would
    /// lose the hue mid-drag.
    hsv: (f32, f32, f32),
    /// The hex field has the keyboard.
    typing: bool,
    /// The hex digits in the field, without the `#`.
    typed: String,
}

impl Picker {
    fn show(&mut self, color: Rgba) {
        self.hsv = rgb_to_hsv(color[0], color[1], color[2]);
        self.typed = to_hex(color)[1..].to_owned();
    }

    /// A typed character: hex digits fill the field, Backspace empties it
    /// one at a time, Enter and Escape give the keyboard back.
    fn key(&mut self, code: u32) {
        if !self.typing {
            return;
        }
        match char::from_u32(code) {
            Some('\u{8}') => {
                self.typed.pop();
            }
            Some('\r' | '\u{1b}') => self.typing = false,
            Some(digit) if digit.is_ascii_hexdigit() && self.typed.len() < 6 => {
                self.typed.push(digit.to_ascii_uppercase());
            }
            _ => {}
        }
    }
}

pub struct View {
    picker: Picker,
    tab: usize,
    /// The section each tab is showing.
    section: [usize; 5],
    scroll: f32,
    /// How far the current tab can scroll, as of the last paint.
    max_scroll: f32,
    mouse: (f32, f32),
    down: bool,
    /// The button went down, or came up, since the last paint.
    pressed: bool,
    released: bool,
    /// The slider being dragged.
    active: Option<u32>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            tab: 0,
            section: [0; 5],
            scroll: 0.0,
            max_scroll: 0.0,
            // Off the panel, so nothing starts out hovered.
            mouse: (-1.0, -1.0),
            down: false,
            pressed: false,
            released: false,
            active: None,
            picker: Picker::default(),
        }
    }
}

impl View {
    /// A view on the General tab with the picker open under its first
    /// colour row, for the snapshot: nothing else can click it open there.
    pub fn with_picker_open() -> Self {
        let mut view = Self::default();
        view.picker.open = Some(0);
        view.picker.show([0.486, 0.361, 1.0, 1.0]);
        view
    }

    /// A view opened on one section of one tab, with the pointer off the panel.
    pub fn on(tab: usize, section: usize) -> Self {
        let mut sections = [0; 5];
        sections[tab] = section;
        Self {
            tab,
            section: sections,
            ..Self::default()
        }
    }

    /// Paints a `w` by `h` panel at the origin and applies whatever the
    /// pointer did to `cfg`. `area` is the work area in display-independent
    /// pixels. The caller owns the frame: this neither begins nor ends it.
    pub fn paint(
        &mut self,
        gfx: &mut Gfx,
        cfg: &mut Config,
        (w, h): (f32, f32),
        accent: Rgba,
        status: Status,
        area: (i32, i32),
    ) -> Result<Outcome> {
        let mut outcome = Outcome::default();
        let font = cfg.theme.font.clone();
        let own_fonts = gfx.own_fonts();
        gfx.fill_rect(rect(0.0, 0.0, w, h), BACKGROUND);
        gfx.fill_rect(rect(0.0, 0.0, SIDEBAR, h), SIDE);

        let view = (HEADER, h);
        let sections = SECTIONS[self.tab];
        let section = self.section[self.tab].min(sections.len() - 1);
        let mut ui = Ui {
            g: &mut *gfx,
            status,
            toggle_hidden: false,
            toggle_autostart: false,
            quit: false,
            font: &font,
            own_fonts: &own_fonts,
            accent,
            left: SIDEBAR + MARGIN,
            width: w - SIDEBAR - 2.0 * MARGIN,
            y: HEADER - self.scroll,
            in_card: false,
            first: true,
            mouse: self.mouse,
            pressed: self.pressed,
            down: self.down,
            released: self.released,
            active: &mut self.active,
            picker: &mut self.picker,
            next_id: 0,
            changed: false,
            view,
            area,
        };
        // The pointer's edges are consumed by this pass whatever happens next.
        self.pressed = false;
        self.released = false;

        // The clip is always popped, even if a row failed to draw: an
        // unbalanced clip would fail the whole frame.
        ui.g.push_clip(rect(SIDEBAR, view.0, w - SIDEBAR, view.1 - view.0));
        let built = match self.tab {
            0 => general(&mut ui, cfg, section),
            1 => clock(&mut ui, cfg, section),
            2 => player(&mut ui, cfg, section),
            3 => lyrics(&mut ui, cfg, section),
            _ => visualizer(&mut ui, cfg, section),
        };
        ui.end_group();
        ui.g.pop_clip();
        built?;
        let content_height = ui.y + self.scroll - HEADER + MARGIN;

        // Everything else sits outside the scrolling area.
        ui.view = (0.0, h);
        let mut turned_to = None;

        // The sidebar: the mark, the tabs, and the two things that leave the panel.
        mark(ui.g, 20.0, 22.0, 28.0)?;
        let name = rect(58.0, 22.0, SIDEBAR - 66.0, 28.0);
        ui.text("Inlay", name, 15.0, 600, Align::Left, TEXT)?;
        for (i, name) in TABS.iter().enumerate() {
            let item = rect(12.0, 74.0 + 40.0 * i as f32, SIDEBAR - 24.0, 36.0);
            let selected = self.tab == i;
            if ui.clicked(item) && !selected {
                turned_to = Some((i, self.section[i]));
            }
            if selected {
                ui.g.fill_round(item, 9.0, with_alpha(BONE, 0.1));
            } else if ui.hot(item) {
                ui.g.fill_round(item, 9.0, with_alpha(BONE, 0.05));
            }
            let color = if selected { TEXT } else { DIM };
            let icon = rect(item.left + 10.0, item.top, 22.0, 36.0);
            ui.glyph(
                TAB_ICONS[i],
                icon,
                15.0,
                if selected { accent } else { DIM },
            )?;
            let label = rect(
                item.left + 42.0,
                item.top,
                item.right - item.left - 50.0,
                36.0,
            );
            let weight = if selected { 600 } else { 400 };
            ui.text(name, label, 13.5, weight, Align::Left, color)?;
        }
        let label = if status.edit {
            "Done editing"
        } else {
            "Edit layout"
        };
        let button = rect(12.0, h - 96.0, SIDEBAR - 24.0, 36.0);
        outcome.toggle_edit = ui.button(label, button, status.edit)?;
        let button = rect(12.0, h - 52.0, SIDEBAR - 24.0, 36.0);
        outcome.open_config = ui.button("Open config file", button, false)?;

        // The page's title and its sections.
        let title = rect(ui.left, 20.0, ui.width, 34.0);
        ui.text(TABS[self.tab], title, 22.0, 700, Align::Left, TEXT)?;
        let strip = rect(ui.left, 64.0, ui.width, 32.0);
        ui.g.fill_round(strip, 9.0, TRACK);
        let cell = ui.width / sections.len() as f32;
        for (i, name) in sections.iter().enumerate() {
            let segment = rect(
                strip.left + cell * i as f32 + 2.0,
                strip.top + 2.0,
                cell - 4.0,
                28.0,
            );
            let selected = section == i;
            if ui.clicked(segment) && !selected {
                turned_to = Some((self.tab, i));
            }
            if selected {
                ui.g.fill_round(segment, 7.0, with_alpha(BONE, 0.16));
            }
            let (weight, color) = if selected { (600, TEXT) } else { (400, DIM) };
            ui.text(name, segment, 12.5, weight, Align::Center, color)?;
        }

        outcome.toggle_hidden = ui.toggle_hidden;
        outcome.toggle_autostart = ui.toggle_autostart;
        outcome.quit = ui.quit;
        outcome.changed = ui.changed;
        if let Some((tab, section)) = turned_to {
            ui.picker.open = None;
            self.tab = tab;
            self.section[tab] = section;
            self.scroll = 0.0;
            outcome.redraw = true;
        }

        let max_scroll = (content_height - (view.1 - view.0)).max(0.0);
        self.max_scroll = max_scroll;
        let clamped = self.scroll.clamp(0.0, max_scroll);
        outcome.redraw |= outcome.changed || clamped != self.scroll;
        self.scroll = clamped;
        Ok(outcome)
    }
}

/// The settings window.
pub struct Panel {
    pub hwnd: HWND,
    surface: Surface,
    size: (u32, u32),
    view: View,
    pub dirty: bool,
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut r) };
    (r.right.max(1) as u32, r.bottom.max(1) as u32)
}

impl Panel {
    pub fn open(gfx: &Gfx, scale: f32) -> Result<Self> {
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: (WIDTH * scale) as i32,
            bottom: (HEIGHT * scale) as i32,
        };
        unsafe { AdjustWindowRectEx(&mut outer, style, false, WINDOW_EX_STYLE(0))? };
        let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
        let (ax, ay, aw, ah) = window::work_area();
        let bounds = (ax + (aw - w) / 2, ay + (ah - h) / 2, w, h);

        let hwnd = window::create_panel(w!("Inlay settings"), style, bounds)?;
        let dark = BOOL(1);
        unsafe {
            // Cosmetic: without it the title bar is white above a dark panel.
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&raw const dark).cast::<c_void>(),
                size_of::<BOOL>() as u32,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
        let size = client_size(hwnd);
        Ok(Self {
            hwnd,
            surface: gfx.surface(hwnd, size.0, size.1)?,
            size,
            view: View::default(),
            dirty: true,
        })
    }

    pub fn close(&self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }

    pub fn focus(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    /// After a graphics device reset.
    pub fn rebuild(&mut self, gfx: &Gfx) -> Result<()> {
        self.surface = gfx.surface(self.hwnd, self.size.0, self.size.1)?;
        self.dirty = true;
        Ok(())
    }

    pub fn mouse(&mut self, kind: Mouse, x: f32, y: f32) {
        let view = &mut self.view;
        match kind {
            Mouse::Down => {
                view.down = true;
                view.pressed = true;
                view.mouse = (x, y);
            }
            Mouse::Up => {
                view.down = false;
                view.released = true;
                view.mouse = (x, y);
            }
            Mouse::Move => view.mouse = (x, y),
            Mouse::Leave => {
                if !view.down {
                    view.mouse = (-1.0, -1.0);
                }
            }
        }
        self.dirty = true;
    }

    /// A typed character, for the colour picker's hex field.
    pub fn key(&mut self, code: u32) {
        self.view.picker.key(code);
        self.dirty = true;
    }

    pub fn wheel(&mut self, delta: i32) {
        // Clamped here, before anything is drawn. Clamping after a paint drew
        // one frame scrolled past the end and the next one snapped back.
        let view = &mut self.view;
        let wanted = (view.scroll - delta as f32 / 120.0 * WHEEL_STEP).clamp(0.0, view.max_scroll);
        if wanted != view.scroll {
            view.scroll = wanted;
            self.dirty = true;
        }
    }

    /// Draws the panel into its window.
    pub fn draw(
        &mut self,
        gfx: &mut Gfx,
        cfg: &mut Config,
        scale: f32,
        accent: Rgba,
        status: Status,
        area: (i32, i32),
    ) -> Result<Outcome> {
        let size = (self.size.0 as f32 / scale, self.size.1 as f32 / scale);
        gfx.begin(&self.surface);
        gfx.set_transform(scale, 0.0, 0.0);
        let painted = self.view.paint(gfx, cfg, size, accent, status, area);
        // The frame is ended whether or not painting succeeded.
        let ended = gfx.end(&self.surface);
        let outcome = painted?;
        ended?;
        self.dirty = outcome.redraw;
        Ok(outcome)
    }
}

/// One frame of the immediate-mode UI.
struct Ui<'a> {
    g: &'a mut Gfx,
    status: Status,
    /// What the General tab's app controls asked for.
    toggle_hidden: bool,
    toggle_autostart: bool,
    quit: bool,
    font: &'a str,
    /// Families loaded from the app's own fonts folder.
    own_fonts: &'a [String],
    accent: Rgba,
    left: f32,
    width: f32,
    /// Top of the next row.
    y: f32,
    /// A card is open, and its next row is its first.
    in_card: bool,
    first: bool,
    mouse: (f32, f32),
    pressed: bool,
    down: bool,
    released: bool,
    active: &'a mut Option<u32>,
    picker: &'a mut Picker,
    next_id: u32,
    changed: bool,
    /// The vertical band in which controls are visible and take input.
    view: (f32, f32),
    area: (i32, i32),
}

impl Ui<'_> {
    fn hot(&self, r: D2D_RECT_F) -> bool {
        let (x, y) = self.mouse;
        x >= r.left && x < r.right && y >= r.top.max(self.view.0) && y < r.bottom.min(self.view.1)
    }

    fn clicked(&self, r: D2D_RECT_F) -> bool {
        self.released && self.active.is_none() && self.hot(r)
    }

    /// Draws one line of text vertically centred in `r`.
    fn text(
        &mut self,
        text: &str,
        r: D2D_RECT_F,
        size: f32,
        weight: u32,
        align: Align,
        color: Rgba,
    ) -> Result<()> {
        let style = TextStyle {
            font: self.font,
            size,
            weight,
            align,
            wrap: false,
        };
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let layout = self.g.layout(text, &style, w, h)?;
        let height = Gfx::measure(&layout).1;
        self.g
            .draw_text(&layout, r.left, r.top + (h - height) / 2.0, color, false);
        Ok(())
    }

    /// One icon from Segoe Fluent Icons, centred in `r`.
    fn glyph(&mut self, glyph: &str, r: D2D_RECT_F, size: f32, color: Rgba) -> Result<()> {
        let style = TextStyle {
            font: ICON_FONT,
            size,
            weight: 400,
            align: Align::Center,
            wrap: false,
        };
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let layout = self.g.layout(glyph, &style, w, h)?;
        let height = Gfx::measure(&layout).1;
        self.g
            .draw_text(&layout, r.left, r.top + (h - height) / 2.0, color, false);
        Ok(())
    }

    /// Starts a card of rows, under a small title when it has one, and ends
    /// the card before it.
    fn group(&mut self, title: &str) -> Result<()> {
        self.end_group();
        self.y += 14.0;
        if !title.is_empty() {
            let line = rect(self.left + 4.0, self.y, self.width - 8.0, 20.0);
            self.y += 26.0;
            self.text(title, line, 12.0, 600, Align::Left, DIM)?;
        }
        // The rounded top. The rows are square pieces of the same colour.
        let cap = rect(self.left, self.y, self.width, 2.0 * CARD_RADIUS);
        self.g.fill_round(cap, CARD_RADIUS, CARD);
        self.y += CARD_RADIUS;
        self.in_card = true;
        self.first = true;
        Ok(())
    }

    fn end_group(&mut self) {
        if !self.in_card {
            return;
        }
        // Only the half below the last row is painted: the rest would
        // cover that row's controls.
        let cap = rect(
            self.left,
            self.y - CARD_RADIUS,
            self.width,
            2.0 * CARD_RADIUS,
        );
        self.g
            .push_clip(rect(self.left, self.y, self.width, CARD_RADIUS));
        self.g.fill_round(cap, CARD_RADIUS, CARD);
        self.g.pop_clip();
        self.y += CARD_RADIUS;
        self.in_card = false;
    }

    /// Claims the next `height` of the page, painting the card behind it,
    /// and a line above it when `divided` and not the card's first.
    /// Returns the part inside the card's sides.
    fn take(&mut self, height: f32, divided: bool) -> D2D_RECT_F {
        let top = self.y;
        self.y += height;
        if !self.in_card {
            return rect(self.left, top, self.width, height);
        }
        // A pixel over at each end: two pieces that only meet leave a seam
        // at a fractional scale.
        self.g
            .fill_rect(rect(self.left, top - 1.0, self.width, height + 2.0), CARD);
        let inner = rect(
            self.left + CARD_INSET,
            top,
            self.width - 2.0 * CARD_INSET,
            height,
        );
        if divided && !self.first {
            self.g.fill_rect(
                rect(inner.left, top, inner.right - inner.left, 1.0),
                HAIRLINE,
            );
        }
        self.first = false;
        inner
    }

    /// Claims the next row. Returns the whole row and the control's part of it.
    fn row(&mut self, label: &str, height: f32) -> Result<(D2D_RECT_F, D2D_RECT_F)> {
        let row = self.take(height, true);
        let width = row.right - row.left;
        let label_width = width * (1.0 - CONTROL_SHARE);
        self.text(
            label,
            rect(row.left, row.top, label_width - 8.0, ROW),
            13.5,
            400,
            Align::Left,
            TEXT,
        )?;
        let control = rect(row.left + label_width, row.top, width - label_width, height);
        Ok((row, control))
    }

    fn button(&mut self, label: &str, r: D2D_RECT_F, lit: bool) -> Result<bool> {
        let fill = if lit {
            with_alpha(self.accent, 0.9)
        } else if self.hot(r) {
            [1.0, 1.0, 1.0, 0.2]
        } else {
            TRACK
        };
        self.g.fill_round(r, 9.0, fill);
        let color = if lit { [0.0, 0.0, 0.0, 0.9] } else { TEXT };
        self.text(label, r, 13.0, 600, Align::Center, color)?;
        Ok(self.clicked(r))
    }

    fn toggle(&mut self, label: &str, value: &mut bool) -> Result<()> {
        let (row, control) = self.row(label, ROW)?;
        if self.clicked(row) {
            *value = !*value;
            self.changed = true;
        }
        let mid = row.top + ROW / 2.0;
        let track = rect(control.right - 40.0, mid - 11.0, 40.0, 22.0);
        let fill = if *value { self.accent } else { TRACK };
        self.g.fill_round(track, 11.0, fill);
        let knob = if *value {
            track.right - 11.0
        } else {
            track.left + 11.0
        };
        self.g.fill_circle(knob, mid, 8.0, [1.0; 4]);
        Ok(())
    }

    /// A slider over `min..=max` that snaps to `step`.
    fn slider(
        &mut self,
        label: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> Result<()> {
        self.slider_with(label, value, (min, max, step), false)
    }

    /// The slider itself. With `quarters` it is notched at every quarter of
    /// its range, and the knob jumps onto a notch it is dragged near.
    fn slider_with(
        &mut self,
        label: &str,
        value: &mut f32,
        (min, max, step): (f32, f32, f32),
        quarters: bool,
    ) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let (row, control) = self.row(label, ROW)?;
        let mid = row.top + ROW / 2.0;
        let track = rect(
            control.left,
            mid - 2.0,
            control.right - control.left - 56.0,
            4.0,
        );
        let length = track.right - track.left;
        let grab = rect(track.left - 8.0, row.top, length + 16.0, ROW);

        if self.pressed && self.active.is_none() && self.hot(grab) {
            *self.active = Some(id);
        }
        if *self.active == Some(id) {
            if self.down || self.released {
                let mut t = ((self.mouse.0 - track.left) / length).clamp(0.0, 1.0);
                let notch = (t * 4.0).round() / 4.0;
                if quarters && ((t - notch) * length).abs() <= SNAP_REACH {
                    t = notch;
                }
                let snapped = (min + ((max - min) * t / step).round() * step).clamp(min, max);
                if snapped != *value {
                    *value = snapped;
                    self.changed = true;
                }
            }
            if !self.down {
                *self.active = None;
            }
        }

        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let knob = track.left + length * t;
        if quarters {
            for quarter in 0..=4 {
                let x = track.left + length * quarter as f32 / 4.0;
                self.g.fill_rect(rect(x - 1.0, mid - 7.0, 2.0, 14.0), TRACK);
            }
        }
        self.g.fill_round(track, 2.0, TRACK);
        self.g.fill_round(
            rect(track.left, track.top, knob - track.left, 4.0),
            2.0,
            self.accent,
        );
        self.g.fill_circle(knob, mid, 7.0, [1.0; 4]);

        let shown = if step >= 1.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.2}")
        };
        let value_box = rect(control.right - 50.0, row.top, 50.0, ROW);
        self.text(&shown, value_box, 12.5, 400, Align::Right, DIM)
    }

    fn slider_u32(
        &mut self,
        label: &str,
        value: &mut u32,
        min: u32,
        max: u32,
        step: u32,
    ) -> Result<()> {
        let mut float = *value as f32;
        self.slider(label, &mut float, min as f32, max as f32, step as f32)?;
        *value = float.round() as u32;
        Ok(())
    }

    /// Steps through `options` with a click on either half of the control.
    fn stepper(
        &mut self,
        label: &str,
        current: Option<usize>,
        labels: &[&str],
    ) -> Result<Option<usize>> {
        let (_, control) = self.row(label, ROW)?;
        let pill = rect(
            control.left,
            control.top + 5.0,
            control.right - control.left,
            ROW - 10.0,
        );
        self.g.fill_round(
            pill,
            9.0,
            if self.hot(pill) {
                [1.0, 1.0, 1.0, 0.2]
            } else {
                TRACK
            },
        );
        let shown = current.map_or("Custom", |i| labels[i]);
        self.text(shown, pill, 13.0, 500, Align::Center, TEXT)?;
        let arrow = |x: f32| rect(x, pill.top, 28.0, pill.bottom - pill.top);
        self.text("‹", arrow(pill.left), 15.0, 400, Align::Center, DIM)?;
        self.text("›", arrow(pill.right - 28.0), 15.0, 400, Align::Center, DIM)?;

        if !self.clicked(pill) || labels.is_empty() {
            return Ok(None);
        }
        let count = labels.len();
        let forward = self.mouse.0 >= (pill.left + pill.right) / 2.0;
        Ok(Some(match (current, forward) {
            (Some(i), true) => (i + 1) % count,
            (Some(i), false) => (i + count - 1) % count,
            (None, _) => 0,
        }))
    }

    /// Picks one of a few values of an enum.
    fn choice<T: Copy + PartialEq>(
        &mut self,
        label: &str,
        value: &mut T,
        options: &[(T, &str)],
    ) -> Result<()> {
        let labels: Vec<&str> = options.iter().map(|(_, name)| *name).collect();
        let current = options.iter().position(|(option, _)| option == value);
        if let Some(index) = self.stepper(label, current, &labels)? {
            *value = options[index].0;
            self.changed = true;
        }
        Ok(())
    }

    /// Picks one of a few preset strings; anything else shows as "Custom".
    fn choice_text(
        &mut self,
        label: &str,
        value: &mut String,
        options: &[(&str, &str)],
    ) -> Result<()> {
        let labels: Vec<&str> = options.iter().map(|(_, name)| *name).collect();
        let current = options
            .iter()
            .position(|(option, _)| *option == value.as_str());
        if let Some(index) = self.stepper(label, current, &labels)? {
            *value = options[index].0.to_owned();
            self.changed = true;
        }
        Ok(())
    }

    /// A colour: named options such as "Accent", a row of swatches, and a
    /// last chip that opens a picker for any colour at all.
    fn color(&mut self, label: &str, value: &mut String, named: &[(&str, &str)]) -> Result<()> {
        // One id for the row, one each for the picker's square and hue bar.
        let id = self.next_id;
        self.next_id += 3;
        let (row, _) = self.row(label, COLOR_ROW)?;
        let mut x = row.left;
        let top = row.top + ROW - 4.0;

        for (option, name) in named {
            let pill = rect(x, top, 58.0, 22.0);
            let selected = value.as_str() == *option;
            if self.button(name, pill, selected)? && !selected {
                *value = (*option).to_owned();
                self.changed = true;
            }
            x += 64.0;
        }
        let mut preset = false;
        for swatch in SWATCHES {
            let chip = rect(x, top, 22.0, 22.0);
            let selected = value.eq_ignore_ascii_case(swatch);
            preset |= selected;
            if self.clicked(chip) && !selected {
                *value = swatch.to_owned();
                self.changed = true;
            }
            if selected {
                self.ring(chip);
            }
            self.g
                .fill_round(chip, 6.0, parse_hex(swatch).unwrap_or([1.0; 4]));
            self.g.stroke_round(chip, 6.0, [1.0, 1.0, 1.0, 0.25], 1.0);
            x += 26.0;
        }

        // The last chip is any other colour. It shows the current one when
        // that is not a swatch, and a spectrum otherwise.
        let chip = rect(x, top, 22.0, 22.0);
        let custom = parse_hex(value).filter(|_| !preset);
        let open = self.picker.open == Some(id);
        if self.clicked(chip) {
            self.picker.open = (!open).then_some(id);
            self.picker.typing = false;
            self.picker
                .show(parse_hex(value).unwrap_or([1.0, 1.0, 1.0, 1.0]));
        }
        match custom {
            Some(color) => self.g.fill_round(chip, 6.0, color),
            None => {
                let spectrum =
                    self.g
                        .gradient(chip.left, chip.right, HUE_STOPS[5], HUE_STOPS[3])?;
                self.g.fill_round_with(chip, 6.0, &spectrum);
            }
        }
        self.g.stroke_round(chip, 6.0, [1.0, 1.0, 1.0, 0.25], 1.0);
        if custom.is_some() || open {
            self.ring(chip);
        }
        if self.picker.open == Some(id) {
            self.picker_body(id, value)?;
        }
        Ok(())
    }

    /// The accent outline around a selected swatch.
    fn ring(&mut self, chip: D2D_RECT_F) {
        let around = rect(chip.left - 2.5, chip.top - 2.5, 27.0, 27.0);
        self.g.stroke_round(around, 8.0, self.accent, 2.0);
    }

    /// The open picker: a square for saturation and brightness, a bar for
    /// hue, and a hex field that takes typing.
    fn picker_body(&mut self, id: u32, value: &mut String) -> Result<()> {
        let (width, height) = PICKER_SQUARE;
        let body = self.take(height + HUE_BAR_HEIGHT + 26.0, false);
        let top = body.top + 2.0;
        let square = rect(body.left, top, width, height);
        let bar = rect(body.left, top + height + 10.0, width, HUE_BAR_HEIGHT);
        let field = rect(
            body.left + width + 18.0,
            top,
            body.right - body.left - width - 18.0,
            34.0,
        );

        // Dragging in the square or along the bar.
        let (mx, my) = self.mouse;
        if self.pressed && self.active.is_none() {
            if self.hot(square) {
                *self.active = Some(id + 1);
            } else if self.hot(rect(bar.left, bar.top - 4.0, width, HUE_BAR_HEIGHT + 8.0)) {
                *self.active = Some(id + 2);
            }
            self.picker.typing = self.hot(field);
        }
        let across = ((mx - square.left) / width).clamp(0.0, 1.0);
        let dragging = *self.active == Some(id + 1) || *self.active == Some(id + 2);
        if dragging {
            if self.down || self.released {
                let (hue, saturation, brightness) = &mut self.picker.hsv;
                if *self.active == Some(id + 1) {
                    *saturation = across;
                    *brightness = 1.0 - ((my - square.top) / height).clamp(0.0, 1.0);
                } else {
                    *hue = across * 359.9;
                }
                let (hue, saturation, brightness) = self.picker.hsv;
                let (r, g, b) = hsv_to_rgb(hue, saturation, brightness);
                let hex = to_hex([r, g, b, 1.0]);
                self.picker.typed = hex[1..].to_owned();
                if hex != *value {
                    *value = hex;
                    self.changed = true;
                }
            }
            if !self.down {
                *self.active = None;
            }
        } else if self.picker.typing {
            // Six digits make a colour; fewer are still being typed.
            if self.picker.typed.len() == 6
                && let Some(color) = parse_hex(&self.picker.typed)
                && !to_hex(color).eq_ignore_ascii_case(value)
            {
                *value = to_hex(color);
                self.picker.hsv = rgb_to_hsv(color[0], color[1], color[2]);
                self.changed = true;
            }
        } else if let Some(color) = parse_hex(value)
            && !self.picker.typed.eq_ignore_ascii_case(&to_hex(color)[1..])
        {
            // The colour was changed some other way, by a swatch say.
            self.picker.show(color);
        }

        // The square: white to the pure hue across, darkening to black downward.
        let (hue, saturation, brightness) = self.picker.hsv;
        let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
        let across_brush = self
            .g
            .gradient(square.left, square.right, [1.0; 4], [r, g, b, 1.0])?;
        self.g.fill_round_with(square, 8.0, &across_brush);
        let down_brush = self.g.vertical_gradient(
            square.top,
            square.bottom,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )?;
        self.g.fill_round_with(square, 8.0, &down_brush);
        let (knob_x, knob_y) = (
            square.left + saturation * width,
            square.top + (1.0 - brightness) * height,
        );
        self.g.fill_circle(knob_x, knob_y, 7.0, [1.0; 4]);
        let (r, g, b) = hsv_to_rgb(hue, saturation, brightness);
        self.g.fill_circle(knob_x, knob_y, 5.0, [r, g, b, 1.0]);

        // The hue bar, one gradient between each pair of primaries.
        let step = width / 6.0;
        for (i, pair) in HUE_STOPS.windows(2).enumerate() {
            let left = bar.left + step * i as f32;
            // A hair of overlap hides the seam between segments.
            let segment = rect(left, bar.top, step + 0.5, HUE_BAR_HEIGHT);
            let brush = self.g.gradient(left, left + step, pair[0], pair[1])?;
            self.g.fill_round_with(segment, 0.0, &brush);
        }
        let hue_x = bar.left + hue / 360.0 * width;
        self.g.fill_round(
            rect(hue_x - 3.0, bar.top - 3.0, 6.0, HUE_BAR_HEIGHT + 6.0),
            3.0,
            [1.0; 4],
        );

        // The hex field, with the colour itself beneath it.
        self.g.fill_round(field, 8.0, TRACK);
        if self.picker.typing {
            self.g.stroke_round(field, 8.0, self.accent, 1.5);
        }
        let caret = if self.picker.typing { "_" } else { "" };
        let shown = format!("#{}{caret}", self.picker.typed);
        self.text(&shown, field, 14.0, 600, Align::Center, TEXT)?;
        let preview = rect(
            field.left,
            field.bottom + 8.0,
            field.right - field.left,
            46.0,
        );
        self.g.fill_round(preview, 8.0, [r, g, b, 1.0]);
        self.g
            .stroke_round(preview, 8.0, [1.0, 1.0, 1.0, 0.25], 1.0);
        let hint = rect(
            field.left,
            preview.bottom + 6.0,
            field.right - field.left,
            34.0,
        );
        self.text(
            "Click the code to type one",
            hint,
            11.5,
            400,
            Align::Center,
            DIM,
        )
    }

    /// Picks a font: the theme's, one from the app's own folder, or one
    /// that ships with Windows.
    fn font_row(&mut self, label: &str, value: &mut String) -> Result<()> {
        let own = self.own_fonts;
        let options: Vec<(&str, &str)> = std::iter::once(("", "Theme font"))
            .chain(own.iter().map(|name| (name.as_str(), name.as_str())))
            .chain(SYSTEM_FONTS.iter().map(|name| (*name, *name)))
            .collect();
        self.choice_text(label, value, &options)
    }

    /// Where something sits on the screen, in the display's own pixels,
    /// notched at every quarter of the screen. `x` and `y` are measured
    /// from the corner of `frame`, the widget the thing belongs to.
    fn position(&mut self, frame: Frame, x: &mut f32, y: &mut f32) -> Result<()> {
        let scale = self.status.scale;
        let (left, top) = frame.origin_within(self.area.0, self.area.1);
        let axes = [
            ("Across (px)", x, left, self.area.0),
            ("Down (px)", y, top, self.area.1),
        ];
        for (label, value, origin, extent) in axes {
            let mut px = ((origin as f32 + *value) * scale).round();
            let before = px;
            let range = (0.0, (extent as f32 * scale).round(), 1.0);
            self.slider_with(label, &mut px, range, true)?;
            if px != before {
                *value = px / scale - origin as f32;
            }
        }
        Ok(())
    }

    /// The same two sliders for a widget with a frame of its own. They
    /// place its centre, so the middle notch is the middle of the screen,
    /// and they keep it on the screen: a frame as tall as the screen has
    /// nowhere to go. `spans` is a frame drawn from edge to edge whatever
    /// its own width says. Returns whether it was moved across.
    fn frame_position(&mut self, frame: &mut Frame, spans: bool) -> Result<bool> {
        let scale = self.status.scale;
        let (area_w, area_h) = self.area;
        let (left, top) = frame.origin_within(area_w, area_h);
        let (w, h) = (frame.w as f32, frame.h as f32);
        let across = if spans {
            area_w as f32 / 2.0
        } else {
            left as f32 + w / 2.0
        };
        let (mut x, mut y) = (
            (across * scale).round(),
            ((top as f32 + h / 2.0) * scale).round(),
        );
        let before = (x, y);
        let range = (0.0, (area_w as f32 * scale).round(), 1.0);
        self.slider_with("Across (px)", &mut x, range, true)?;
        let range = (0.0, (area_h as f32 * scale).round(), 1.0);
        self.slider_with("Down (px)", &mut y, range, true)?;

        let corner = |centre: f32, size: f32, room: i32| {
            let wanted = (centre / scale - size / 2.0).round() as i32;
            wanted.clamp(0, (room - size as i32).max(0))
        };
        let moved = (x != before.0, y != before.1);
        if moved.0 || moved.1 {
            let left = if moved.0 { corner(x, w, area_w) } else { left };
            let top = if moved.1 { corner(y, h, area_h) } else { top };
            frame.set_origin(left, top, area_w, area_h);
            frame.reanchor(area_w, area_h);
        }
        Ok(moved.0)
    }

    /// Every setting of one piece of text: where, how it looks, which way
    /// it runs.
    fn label_page(&mut self, frame: Frame, label: &mut Label) -> Result<()> {
        self.group("Position")?;
        self.position(frame, &mut label.x, &mut label.y)?;
        let aligns = if label.direction == Direction::Horizontal {
            &ALIGNS
        } else {
            &RUNNING_ALIGNS
        };
        self.choice("Grows from its", &mut label.align, aligns)?;

        self.group("Text")?;
        self.font_row("Font", &mut label.font)?;
        self.slider("Size", &mut label.size, 6.0, 240.0, 1.0)?;
        self.slider_u32("Weight", &mut label.weight, 100, 900, 100)?;
        self.color("Colour", &mut label.color, &THEME_COLORS)?;
        self.slider("Opacity", &mut label.opacity, 0.0, 1.0, 0.02)?;
        self.slider("Letter spacing", &mut label.spacing, 0.0, 1.0, 0.02)?;
        self.toggle("Capitals", &mut label.uppercase)?;

        self.group("Direction")?;
        self.choice("Runs", &mut label.direction, &DIRECTIONS)?;
        self.choice("Letters", &mut label.letters, &LETTERS)
    }

    /// A labelled row with one button. Returns whether it was clicked.
    fn action(&mut self, label: &str, button: &str) -> Result<bool> {
        let (_, control) = self.row(label, ROW)?;
        let pill = rect(
            control.left,
            control.top + 5.0,
            control.right - control.left,
            ROW - 10.0,
        );
        self.button(button, pill, false)
    }

    /// A line of small print.
    fn note(&mut self, text: &str) -> Result<()> {
        let line = self.take(28.0, false);
        let line = rect(line.left, line.top, line.right - line.left, 24.0);
        self.text(text, line, 12.0, 400, Align::Left, DIM)
    }
}

fn general(ui: &mut Ui, cfg: &mut Config, section: usize) -> Result<()> {
    match section {
        0 => {
            ui.group("Looks")?;
            let strip = ui.take(46.0, false);
            let width = (strip.right - strip.left - 30.0) / 4.0;
            for (i, preset) in Preset::ALL.into_iter().enumerate() {
                let left = strip.left + (width + 10.0) * i as f32;
                let button = rect(left, strip.top + 6.0, width, 34.0);
                if ui.button(preset.name(), button, false)? {
                    cfg.apply_preset(preset);
                    ui.changed = true;
                }
            }

            ui.group("Theme")?;
            let theme = &mut cfg.theme;
            ui.choice_text("Font", &mut theme.font, &FONTS)?;
            ui.color("Accent", &mut theme.accent, &[("auto", "Album")])?;
            ui.color("Text", &mut theme.text, &[])?;
            ui.toggle("Shadow under text", &mut theme.text_shadow)?;

            ui.group("Cards behind widgets")?;
            ui.color("Colour", &mut theme.card_color, &[])?;
            ui.slider("Opacity", &mut theme.card_opacity, 0.0, 1.0, 0.02)?;
            ui.slider("Corner", &mut theme.card_radius, 0.0, 40.0, 1.0)?;
            ui.slider("Border", &mut theme.card_border, 0.0, 0.5, 0.01)
        }
        1 => {
            ui.group("")?;
            let general = &mut cfg.general;
            ui.choice(
                "Layer",
                &mut general.layer,
                &[
                    (Layer::Desktop, "On the desktop"),
                    (Layer::Normal, "Normal window"),
                    (Layer::Top, "Always on top"),
                ],
            )?;
            ui.toggle(
                "Pause behind fullscreen apps",
                &mut general.pause_on_fullscreen,
            )?;
            ui.toggle(
                "Hide when Spotify is closed",
                &mut general.hide_without_spotify,
            )?;
            ui.toggle("Scale widgets with the screen", &mut general.fit_screen)?;
            ui.toggle("Global hotkeys", &mut general.hotkeys)
        }
        _ => {
            ui.group("")?;
            let (mut hidden, mut autostart) = (ui.status.hidden, ui.status.autostart);
            ui.toggle("Hide all widgets", &mut hidden)?;
            ui.toggle("Start with Windows", &mut autostart)?;
            ui.toggle_hidden = hidden != ui.status.hidden;
            ui.toggle_autostart = autostart != ui.status.autostart;
            if ui.action("Inlay", "Quit")? {
                ui.quit = true;
            }
            Ok(())
        }
    }
}

fn clock(ui: &mut Ui, cfg: &mut Config, section: usize) -> Result<()> {
    let c = &mut cfg.clock;
    let frame = c.frame;
    ui.group("")?;
    let (show, format, formats, label) = match section {
        0 => {
            ui.toggle("Show", &mut c.enabled)?;
            ui.toggle("Card behind it", &mut c.card)?;
            ui.slider("Opacity", &mut c.opacity, 0.0, 1.0, 0.05)?;
            if ui.action("Day, time and date", "Put back in place")? {
                c.reset_positions();
                ui.changed = true;
            }
            return Ok(());
        }
        1 => (
            &mut c.show_day,
            &mut c.day_format,
            &DAY_FORMATS[..],
            &mut c.day,
        ),
        2 => (
            &mut c.show_time,
            &mut c.time_format,
            &TIME_FORMATS[..],
            &mut c.time,
        ),
        _ => (
            &mut c.show_date,
            &mut c.date_format,
            &DATE_FORMATS[..],
            &mut c.date,
        ),
    };
    ui.toggle("Show", show)?;
    ui.choice_text("Shows", format, formats)?;
    ui.label_page(frame, label)
}

fn player(ui: &mut Ui, cfg: &mut Config, section: usize) -> Result<()> {
    let p = &mut cfg.player;
    let frame = p.frame;
    match section {
        0 => {
            ui.group("")?;
            ui.toggle("Show", &mut p.enabled)?;
            ui.choice(
                "Background",
                &mut p.background,
                &[
                    (PlayerBackground::ArtBlur, "Blurred album art"),
                    (PlayerBackground::Card, "Card"),
                    (PlayerBackground::None, "None"),
                ],
            )?;
            ui.slider("Opacity", &mut p.opacity, 0.0, 1.0, 0.05)?;

            ui.group("Arrange everything at once")?;
            for (layout, label) in [
                (PlayerLayout::Row, "Art on the left"),
                (PlayerLayout::Centered, "Centred stack"),
            ] {
                if ui.action(label, "Arrange")? {
                    p.arrange(layout);
                    ui.changed = true;
                }
            }
            Ok(())
        }
        1 => {
            ui.group("")?;
            ui.toggle("Show", &mut p.show_art)?;
            ui.slider("Size", &mut p.art_size, 16.0, 600.0, 2.0)?;
            ui.slider("Corner rounding", &mut p.art_radius, 0.0, 0.5, 0.02)?;
            ui.group("Position")?;
            ui.position(frame, &mut p.art.x, &mut p.art.y)
        }
        2 => {
            ui.group("")?;
            ui.toggle("Song name only", &mut p.short_title)?;
            ui.note("Leaves out the version, the credits and the film.")?;
            ui.label_page(frame, &mut p.title)
        }
        3 => {
            ui.group("")?;
            ui.slider(
                "Room before it is cut",
                &mut p.text_width,
                60.0,
                1200.0,
                10.0,
            )?;
            ui.label_page(frame, &mut p.artist)
        }
        4 => {
            ui.group("")?;
            ui.toggle("Show", &mut p.show_controls)?;
            ui.slider("Size", &mut p.button_size, 14.0, 120.0, 1.0)?;
            ui.color("Colour", &mut p.button_color, &THEME_COLORS)?;
            for (name, spot) in [
                ("Previous", &mut p.previous),
                ("Play and pause", &mut p.play),
                ("Next", &mut p.next),
            ] {
                ui.group(name)?;
                ui.position(frame, &mut spot.x, &mut spot.y)?;
            }
            Ok(())
        }
        5 => {
            ui.group("")?;
            ui.toggle("Show bar and times", &mut p.show_progress)?;
            ui.slider("Length", &mut p.bar_width, 20.0, 1200.0, 2.0)?;
            ui.slider("Thickness", &mut p.bar_height, 1.0, 40.0, 1.0)?;
            ui.color("Colour", &mut p.bar_color, &THEME_COLORS)?;
            ui.slider("Unplayed part", &mut p.bar_track_opacity, 0.0, 1.0, 0.02)?;
            ui.group("Position")?;
            ui.position(frame, &mut p.bar.x, &mut p.bar.y)
        }
        6 => ui.label_page(frame, &mut p.elapsed),
        _ => ui.label_page(frame, &mut p.total),
    }
}

fn lyrics(ui: &mut Ui, cfg: &mut Config, section: usize) -> Result<()> {
    let l = &mut cfg.lyrics;
    match section {
        0 => {
            ui.group("")?;
            ui.toggle("Show", &mut l.enabled)?;
            ui.choice(
                "What is being sung",
                &mut l.mode,
                &[
                    (LyricsMode::Complete, "Lights up at once"),
                    (LyricsMode::Progress, "Fills as it is sung"),
                ],
            )?;
            ui.toggle("Word by word where available", &mut l.word_sync)?;
            ui.note("Some songs have timed words. The rest are followed line by line.")?;
            ui.toggle("Card behind them", &mut l.card)?;
            ui.slider("Opacity", &mut l.opacity, 0.0, 1.0, 0.05)?;

            ui.group("Where its middle is, and its size")?;
            ui.frame_position(&mut l.frame, false)?;
            let (max_w, max_h) = (ui.area.0.max(80) as u32, ui.area.1.max(40) as u32);
            ui.slider_u32("Width", &mut l.frame.w, 80, max_w, 10)?;
            ui.slider_u32("Height", &mut l.frame.h, 40, max_h, 10)
        }
        1 => {
            ui.group("")?;
            ui.choice("Align", &mut l.align, &ALIGNS)?;
            ui.font_row("Font", &mut l.font)?;
            ui.slider("Size", &mut l.size, 12.0, 96.0, 1.0)?;
            ui.slider_u32("Weight", &mut l.weight, 100, 900, 100)?;
            ui.slider("Current line size", &mut l.active_scale, 1.0, 1.6, 0.02)?;

            ui.group("Lines")?;
            ui.slider_u32("Above", &mut l.lines_before, 0, 6, 1)?;
            ui.slider_u32("Below", &mut l.lines_after, 0, 6, 1)?;
            ui.slider("Spacing", &mut l.line_gap, 0.0, 2.0, 0.05)?;
            ui.slider("Fade with distance", &mut l.falloff, 0.0, 0.9, 0.05)
        }
        2 => {
            ui.group("Current line")?;
            ui.color("Colour", &mut l.active_color, &THEME_COLORS)?;
            ui.color("Word being sung", &mut l.word_color, &THEME_COLORS)?;
            ui.slider("Unsung part", &mut l.unsung_opacity, 0.0, 1.0, 0.02)?;

            ui.group("Other lines")?;
            ui.color("Colour", &mut l.inactive_color, &THEME_COLORS)?;
            ui.slider("Opacity", &mut l.inactive_opacity, 0.0, 1.0, 0.02)?;

            ui.group("Outline and shadow")?;
            ui.slider("Outline width", &mut l.stroke_width, 0.0, 6.0, 0.5)?;
            ui.color("Outline colour", &mut l.stroke_color, &[])?;
            ui.slider("Shadow strength", &mut l.shadow_opacity, 0.0, 1.0, 0.02)?;
            ui.color("Shadow colour", &mut l.shadow_color, &[])
        }
        _ => {
            ui.group("")?;
            let mut offset = l.offset_ms as f32;
            ui.slider("Show earlier (ms)", &mut offset, -3000.0, 3000.0, 50.0)?;
            l.offset_ms = offset as i32;
            ui.slider_u32("Scroll time (ms)", &mut l.scroll_ms, 0, 1000, 20)
        }
    }
}

fn visualizer(ui: &mut Ui, cfg: &mut Config, section: usize) -> Result<()> {
    let v = &mut cfg.visualizer;
    match section {
        0 => {
            ui.group("")?;
            ui.toggle("Show", &mut v.enabled)?;
            ui.choice(
                "Listens to",
                &mut v.source,
                &[
                    (AudioSource::Spotify, "Spotify only"),
                    (AudioSource::System, "Everything"),
                ],
            )?;
            ui.toggle("Card behind it", &mut v.card)?;
            ui.slider("Opacity", &mut v.opacity, 0.0, 1.0, 0.05)?;

            ui.group("Where its middle is, and its size")?;
            ui.toggle("Span the whole screen", &mut v.full_width)?;
            if ui.frame_position(&mut v.frame, v.full_width)? {
                // Moved across: it is an ordinary box from here on.
                v.full_width = false;
            }
            let (max_w, max_h) = (ui.area.0.max(80) as u32, ui.area.1.max(40) as u32);
            if !v.full_width {
                ui.slider_u32("Width", &mut v.frame.w, 80, max_w, 10)?;
            }
            ui.slider_u32("Height", &mut v.frame.h, 40, max_h, 10)?;
            let turned = v.rotation;
            ui.choice(
                "Turned",
                &mut v.rotation,
                &[
                    (0, "Not turned"),
                    (90, "A quarter"),
                    (180, "Upside down"),
                    (270, "Three quarters"),
                ],
            )?;
            if turned / 90 % 2 != v.rotation / 90 % 2 {
                // Stood up or laid down: the box turns with its bars.
                v.full_width = false;
                std::mem::swap(&mut v.frame.w, &mut v.frame.h);
            }
            Ok(())
        }
        1 => {
            ui.group("")?;
            ui.choice(
                "Style",
                &mut v.style,
                &[
                    (VisualizerStyle::Bars, "Bars"),
                    (VisualizerStyle::Mirror, "Mirrored bars"),
                    (VisualizerStyle::Wave, "Wave"),
                ],
            )?;
            ui.slider_u32("Bars", &mut v.bars, 4, MAX_BARS, 1)?;
            ui.slider("Bar width", &mut v.bar_width, 1.0, 60.0, 1.0)?;
            ui.slider("Gap", &mut v.bar_gap, 0.0, 60.0, 1.0)?;
            ui.note("Bars that do not fit are left out; they are never squeezed.")?;
            ui.toggle("Dots for bars at rest", &mut v.show_idle)?;
            ui.slider("Roundness", &mut v.radius, 0.0, 0.5, 0.05)?;

            ui.group("Order")?;
            ui.toggle("Bass in the middle", &mut v.symmetric)?;
            ui.toggle("Flip left to right", &mut v.flip_x)?;
            ui.toggle("Flip upside down", &mut v.flip_y)
        }
        2 => {
            ui.group("")?;
            ui.choice(
                "Coloured by",
                &mut v.color,
                &[
                    (VisualizerColor::Accent, "Accent"),
                    (VisualizerColor::Solid, "One colour"),
                    (VisualizerColor::Gradient, "Gradient"),
                ],
            )?;
            ui.color("First colour", &mut v.color_a, &[])?;
            ui.color("Second colour", &mut v.color_b, &[])
        }
        _ => {
            ui.group("")?;
            ui.slider("Sensitivity (dB)", &mut v.sensitivity, -20.0, 30.0, 1.0)?;
            ui.slider("Rise (ms)", &mut v.attack_ms, 0.0, 200.0, 2.0)?;
            ui.slider("Fall (ms)", &mut v.decay_ms, 20.0, 1000.0, 10.0)?;
            ui.slider_u32("Frames per second", &mut v.fps, 15, 144, 1)?;

            ui.group("Pitch")?;
            ui.slider("Treble boost", &mut v.tilt, 0.0, 9.0, 0.5)?;
            ui.slider("Lowest (Hz)", &mut v.min_hz, 20.0, 500.0, 10.0)?;
            ui.slider("Highest (Hz)", &mut v.max_hz, 4000.0, 20_000.0, 500.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typing(start: &str) -> Picker {
        Picker {
            typing: true,
            typed: start.to_owned(),
            ..Picker::default()
        }
    }

    #[test]
    fn the_hex_field_takes_six_hex_digits_and_nothing_else() {
        let mut picker = typing("");
        for ch in "#1d b9zz54ff".chars() {
            picker.key(ch as u32);
        }
        assert_eq!(picker.typed, "1DB954");
    }

    #[test]
    fn backspace_removes_a_digit_and_enter_gives_the_keyboard_back() {
        let mut picker = typing("1DB954");
        picker.key(8);
        assert_eq!(picker.typed, "1DB95");
        picker.key(13);
        assert!(!picker.typing);
        // No longer typing: keys are ignored.
        picker.key('A' as u32);
        assert_eq!(picker.typed, "1DB95");
    }

    #[test]
    fn showing_a_colour_fills_the_field_and_keeps_its_hue() {
        let mut picker = Picker::default();
        picker.show([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(picker.typed, "FF0000");
        assert_eq!(picker.hsv, (0.0, 1.0, 1.0));
    }
}
