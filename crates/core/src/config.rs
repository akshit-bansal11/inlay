//! The settings file. Every field has a default, so a partial or outdated
//! file still loads, and unknown keys are ignored.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub theme: Theme,
    pub clock: ClockCfg,
    pub player: PlayerCfg,
    pub lyrics: LyricsCfg,
    pub visualizer: VisualizerCfg,
    pub stats: StatsCfg,
}

/// Where the widgets sit among other windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    /// On the wallpaper, under every window.
    #[default]
    Desktop,
    /// An ordinary window: whatever is clicked covers it.
    Normal,
    /// Above everything.
    Top,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub layer: Layer,
    /// Stop drawing while a fullscreen app or game is in front.
    pub pause_on_fullscreen: bool,
    /// Hide the player, lyrics and visualizer while Spotify is not running.
    pub hide_without_spotify: bool,
    /// Global hotkeys: Ctrl+Alt+E edit layout, Ctrl+Alt+H hide,
    /// Ctrl+Alt+S settings, Ctrl+Alt+Up/Down lyric offset.
    pub hotkeys: bool,
    /// Grow or shrink every widget with the screen, so a layout made on a
    /// big monitor keeps its proportions on a laptop.
    pub fit_screen: bool,
    /// How tall the work area was, in display-independent pixels, when the
    /// layout was first laid out. 0 until the app has seen a screen.
    pub layout_height: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            layer: Layer::Desktop,
            pause_on_fullscreen: true,
            hide_without_spotify: true,
            hotkeys: true,
            fit_screen: true,
            layout_height: 0,
        }
    }
}

impl General {
    /// How much the widgets are grown or shrunk on a work area `area_h`
    /// display-independent pixels tall.
    pub fn layout_scale(&self, area_h: f32) -> f32 {
        if !self.fit_screen || self.layout_height == 0 || !area_h.is_finite() || area_h <= 0.0 {
            return 1.0;
        }
        (area_h / self.layout_height as f32).clamp(0.25, 4.0)
    }
}

/// Colours are `#RRGGBB` or `#RRGGBBAA`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub font: String,
    /// `"auto"` takes the colour from the album art; otherwise a hex colour.
    pub accent: String,
    pub text: String,
    pub card_color: String,
    pub card_opacity: f32,
    pub card_radius: f32,
    /// Opacity of the hairline border around cards. 0 removes it.
    pub card_border: f32,
    /// A soft dark shadow under text, so it reads on a bright wallpaper.
    pub text_shadow: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: "Segoe UI Variable Display".to_owned(),
            accent: "auto".to_owned(),
            text: "#FFFFFF".to_owned(),
            card_color: "#101014".to_owned(),
            card_opacity: 0.62,
            card_radius: 22.0,
            card_border: 0.10,
            text_shadow: true,
        }
    }
}

/// The screen corner or edge a widget's position is measured from, so a
/// layout survives a resolution change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// A widget's place on screen, in display-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Frame {
    pub anchor: Anchor,
    /// Inward distance from the anchor's edge; for a centred axis, an offset from centre.
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new(Anchor::TopLeft, 48, 48, 400, 160)
    }
}

impl Frame {
    pub const fn new(anchor: Anchor, x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { anchor, x, y, w, h }
    }

    /// Top-left corner inside a work area of `area_w` by `area_h`.
    pub fn origin(&self, area_w: i32, area_h: i32) -> (i32, i32) {
        use Anchor::*;
        let (w, h) = (self.w as i32, self.h as i32);
        let x = match self.anchor {
            TopLeft | Left | BottomLeft => self.x,
            Top | Center | Bottom => (area_w - w) / 2 + self.x,
            TopRight | Right | BottomRight => area_w - w - self.x,
        };
        let y = match self.anchor {
            TopLeft | Top | TopRight => self.y,
            Left | Center | Right => (area_h - h) / 2 + self.y,
            BottomLeft | Bottom | BottomRight => area_h - h - self.y,
        };
        (x, y)
    }

    /// The inverse of [`Frame::origin`]: after a drag to `(left, top)`, the
    /// offsets that keep the current anchor.
    pub fn set_origin(&mut self, left: i32, top: i32, area_w: i32, area_h: i32) {
        let (x, y) = self.origin(area_w, area_h);
        // Moving an offset by one moves the origin by +1 or -1, depending on
        // which edge the anchor measures from.
        let probe = Frame {
            x: self.x + 1,
            y: self.y + 1,
            ..*self
        };
        let (px, py) = probe.origin(area_w, area_h);
        self.x += (left - x) * (px - x);
        self.y += (top - y) * (py - y);
    }

    /// Like [`Frame::origin`], but pulled back inside the work area, so a
    /// layout made on a bigger screen never leaves a widget off the edge.
    pub fn origin_within(&self, area_w: i32, area_h: i32) -> (i32, i32) {
        let (x, y) = self.origin(area_w, area_h);
        (
            x.clamp(0, (area_w - self.w as i32).max(0)),
            y.clamp(0, (area_h - self.h as i32).max(0)),
        )
    }

    /// Measures the frame from whichever edge, corner or centre it sits
    /// nearest, without moving it. A widget left in the middle then stays
    /// in the middle on a screen of another size, and one by the right
    /// edge stays by the right edge.
    pub fn reanchor(&mut self, area_w: i32, area_h: i32) {
        use Anchor::*;
        let (left, top) = self.origin(area_w, area_h);
        let third = |start: i32, size: u32, room: i32| {
            let centre = start + size as i32 / 2;
            if centre * 3 < room {
                0
            } else if centre * 3 > room * 2 {
                2
            } else {
                1
            }
        };
        self.anchor = match (third(left, self.w, area_w), third(top, self.h, area_h)) {
            (0, 0) => TopLeft,
            (1, 0) => Top,
            (2, 0) => TopRight,
            (0, 1) => Left,
            (1, 1) => Center,
            (2, 1) => Right,
            (0, _) => BottomLeft,
            (1, _) => Bottom,
            _ => BottomRight,
        };
        self.set_origin(left, top, area_w, area_h);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Which way a line of text runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    #[default]
    Horizontal,
    /// Top to bottom.
    Down,
    /// Bottom to top.
    Up,
}

/// How the letters stand in their line, whichever way it runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Letters {
    #[default]
    Upright,
    /// Turned a quarter, lying along the line.
    Sideways,
}

/// Where one element sits inside its widget, in display-independent pixels
/// from the widget's top-left corner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
}

impl Spot {
    pub const fn at(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

fn sanitize_position(x: &mut f32, y: &mut f32) {
    for value in [x, y] {
        *value = if value.is_finite() {
            value.clamp(-4000.0, 8000.0)
        } else {
            0.0
        };
    }
}

/// One piece of text: where it sits and everything about how it looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Label {
    pub x: f32,
    pub y: f32,
    /// Which edge of the text `x` marks. `center` keeps a title centred on
    /// `x` whatever its length.
    pub align: Align,
    /// Empty, or a font that is not available, uses the theme font.
    pub font: String,
    pub size: f32,
    /// 100 to 900.
    pub weight: u32,
    /// `"text"`, `"accent"`, or a hex colour.
    pub color: String,
    pub opacity: f32,
    /// Extra space after every letter, as a fraction of the size.
    pub spacing: f32,
    pub uppercase: bool,
    pub direction: Direction,
    pub letters: Letters,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            align: Align::Left,
            font: String::new(),
            size: 16.0,
            weight: 400,
            color: "text".to_owned(),
            opacity: 1.0,
            spacing: 0.0,
            uppercase: false,
            direction: Direction::Horizontal,
            letters: Letters::Upright,
        }
    }
}

impl Label {
    fn sized(size: f32, weight: u32) -> Self {
        Self {
            size,
            weight,
            ..Self::default()
        }
    }

    fn sanitize(&mut self) {
        sanitize_position(&mut self.x, &mut self.y);
        self.weight = self.weight.clamp(100, 900);
        for (value, lo, hi, fallback) in [
            (&mut self.size, 6.0, 400.0, 16.0),
            (&mut self.opacity, 0.0, 1.0, 1.0),
            (&mut self.spacing, 0.0, 2.0, 0.0),
        ] {
            *value = if value.is_finite() {
                value.clamp(lo, hi)
            } else {
                fallback
            };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockRow {
    Day,
    Time,
    Date,
}

impl ClockRow {
    pub const ALL: [ClockRow; 3] = [ClockRow::Day, ClockRow::Time, ClockRow::Date];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    /// See `timefmt::format` for the specifiers.
    pub day_format: String,
    pub time_format: String,
    pub date_format: String,
    pub show_day: bool,
    pub show_time: bool,
    pub show_date: bool,
    pub day: Label,
    pub time: Label,
    pub date: Label,
    pub frame: Frame,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::TopLeft, 56, 48, 460, 200),
            card: false,
            opacity: 1.0,
            day_format: "%A".to_owned(),
            time_format: "%H:%M".to_owned(),
            date_format: "%e %B %Y".to_owned(),
            show_day: true,
            show_time: true,
            show_date: true,
            // Anurati and Quicksand: the pairing the Mond Rainmeter skin uses.
            day: Label {
                x: 14.0,
                y: 8.0,
                font: "Anurati".to_owned(),
                color: "accent".to_owned(),
                spacing: 0.16,
                uppercase: true,
                ..Label::sized(34.0, 600)
            },
            time: Label {
                x: 14.0,
                y: 36.0,
                font: "Quicksand".to_owned(),
                ..Label::sized(96.0, 300)
            },
            date: Label {
                x: 14.0,
                y: 146.0,
                font: "Quicksand".to_owned(),
                opacity: 0.74,
                ..Label::sized(20.0, 400)
            },
        }
    }
}

impl ClockCfg {
    pub fn label(&self, row: ClockRow) -> &Label {
        match row {
            ClockRow::Day => &self.day,
            ClockRow::Time => &self.time,
            ClockRow::Date => &self.date,
        }
    }

    pub fn label_mut(&mut self, row: ClockRow) -> &mut Label {
        match row {
            ClockRow::Day => &mut self.day,
            ClockRow::Time => &mut self.time,
            ClockRow::Date => &mut self.date,
        }
    }

    /// Puts the three rows back where they start, keeping how they look.
    pub fn reset_positions(&mut self) {
        let fresh = ClockCfg::default();
        for row in ClockRow::ALL {
            let (from, to) = (fresh.label(row), self.label_mut(row));
            (to.x, to.y, to.align) = (from.x, from.y, from.align);
        }
    }
}

/// How the System widget's readings are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsStructure {
    /// One line of readings.
    #[default]
    Strip,
    /// One column, every cell as wide as the widest.
    List,
    /// A grid of equal cells.
    Tiles,
    /// Each reading where `x` and `y` put it.
    Free,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsOrientation {
    #[default]
    Across,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsMeter {
    #[default]
    None,
    Bar,
    Ring,
}

/// Where a reading's name goes relative to its value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsNames {
    #[default]
    Before,
    Above,
    After,
    Hidden,
}

/// Like [`Layer`], with a way to follow the general setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatsLayer {
    #[default]
    Inherit,
    Desktop,
    Normal,
    Top,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetUnits {
    #[default]
    Bytes,
    Bits,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValueFormat {
    #[default]
    Percent,
    /// Used out of total, such as `9.3 / 16 GB`.
    Amount,
}

/// One thing the System widget can show. The discriminant is the reading's
/// part id and its index in [`StatReading::ALL`].
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatReading {
    Cpu = 0,
    Ram = 1,
    Gpu = 2,
    Vram = 3,
    Disks = 4,
    Down = 5,
    Up = 6,
    Battery = 7,
}

impl StatReading {
    pub const ALL: [StatReading; 8] = [
        StatReading::Cpu,
        StatReading::Ram,
        StatReading::Gpu,
        StatReading::Vram,
        StatReading::Disks,
        StatReading::Down,
        StatReading::Up,
        StatReading::Battery,
    ];
}

/// One reading of the System widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    pub show: bool,
    /// The disks reading's name may hold `{drive}`.
    pub name: String,
    /// Empty uses the widget's meter colour; otherwise `"text"`, `"accent"`
    /// or a hex colour.
    pub color: String,
    /// Percent from which the reading turns the warning colour; 0 never. The
    /// battery warns at or below it, while not charging.
    pub warn: f32,
    pub value: ValueFormat,
    /// The rate that fills a network meter; 0 uses the fastest seen.
    pub max: f64,
    /// Where the reading sits in the free structure.
    pub x: f32,
    pub y: f32,
}

impl Reading {
    fn new(name: &str, warn: f32, slot: u8) -> Self {
        Self {
            show: true,
            name: name.to_owned(),
            color: String::new(),
            warn,
            value: ValueFormat::Percent,
            max: 0.0,
            x: 14.0,
            y: 14.0 + 30.0 * f32::from(slot),
        }
    }
}

impl Default for Reading {
    fn default() -> Self {
        Self::new("", 0.0, 0)
    }
}

/// The System widget. Only font, size, weight, color, opacity, spacing and
/// uppercase of its two labels are used: `x`, `y`, `align`, `direction` and
/// `letters` are ignored, the layout places the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatsCfg {
    pub enabled: bool,
    pub structure: StatsStructure,
    pub orientation: StatsOrientation,
    /// Cells per row (across) or per column (down) in tiles.
    pub columns: u32,
    pub meter: StatsMeter,
    /// Length of a bar; 0 is as wide as its reading.
    pub meter_length: f32,
    pub meter_thickness: f32,
    pub ring_size: f32,
    pub meter_color: String,
    pub track_opacity: f32,
    pub names: StatsNames,
    pub refresh_ms: u32,
    pub gap: f32,
    pub padding: f32,
    pub card: bool,
    pub opacity: f32,
    pub layer: StatsLayer,
    pub warn_color: String,
    pub net_units: NetUnits,
    /// Empty counts every adapter; otherwise only those whose name contains this.
    pub net_adapter: String,
    /// Empty shows every drive; otherwise the letters listed, such as `"CD"`.
    pub drives: String,
    /// Left to right, or top to bottom.
    pub order: Vec<StatReading>,
    pub frame: Frame,
    /// Only font, size, weight, color, opacity, spacing and uppercase are
    /// used; `x`, `y`, `align`, `direction` and `letters` are ignored.
    pub name: Label,
    /// Only font, size, weight, color, opacity, spacing and uppercase are
    /// used; `x`, `y`, `align`, `direction` and `letters` are ignored.
    pub value: Label,
    pub cpu: Reading,
    pub ram: Reading,
    pub gpu: Reading,
    pub vram: Reading,
    pub disks: Reading,
    pub down: Reading,
    pub up: Reading,
    pub battery: Reading,
}

impl Default for StatsCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            structure: StatsStructure::Strip,
            orientation: StatsOrientation::Across,
            columns: 4,
            meter: StatsMeter::None,
            meter_length: 0.0,
            meter_thickness: 4.0,
            ring_size: 28.0,
            meter_color: "accent".to_owned(),
            track_opacity: 0.18,
            names: StatsNames::Before,
            refresh_ms: 1000,
            gap: 18.0,
            padding: 14.0,
            card: false,
            opacity: 1.0,
            layer: StatsLayer::Inherit,
            warn_color: "#FFC93C".to_owned(),
            net_units: NetUnits::Bytes,
            net_adapter: String::new(),
            drives: String::new(),
            order: StatReading::ALL.to_vec(),
            frame: Frame::new(Anchor::TopRight, 56, 48, 640, 40),
            name: Label {
                opacity: 0.66,
                ..Label::sized(13.0, 400)
            },
            value: Label::sized(15.0, 600),
            cpu: Reading::new("CPU", 90.0, 0),
            ram: Reading::new("RAM", 90.0, 1),
            gpu: Reading::new("GPU", 90.0, 2),
            vram: Reading::new("VRAM", 90.0, 3),
            disks: Reading::new("{drive}", 90.0, 4),
            down: Reading::new("↓", 0.0, 5),
            up: Reading::new("↑", 0.0, 6),
            battery: Reading::new("BAT", 20.0, 7),
        }
    }
}

impl StatsCfg {
    pub fn reading(&self, r: StatReading) -> &Reading {
        match r {
            StatReading::Cpu => &self.cpu,
            StatReading::Ram => &self.ram,
            StatReading::Gpu => &self.gpu,
            StatReading::Vram => &self.vram,
            StatReading::Disks => &self.disks,
            StatReading::Down => &self.down,
            StatReading::Up => &self.up,
            StatReading::Battery => &self.battery,
        }
    }

    pub fn reading_mut(&mut self, r: StatReading) -> &mut Reading {
        match r {
            StatReading::Cpu => &mut self.cpu,
            StatReading::Ram => &mut self.ram,
            StatReading::Gpu => &mut self.gpu,
            StatReading::Vram => &mut self.vram,
            StatReading::Disks => &mut self.disks,
            StatReading::Down => &mut self.down,
            StatReading::Up => &mut self.up,
            StatReading::Battery => &mut self.battery,
        }
    }

    pub fn resolved_layer(&self, general: &General) -> Layer {
        match self.layer {
            StatsLayer::Inherit => general.layer,
            StatsLayer::Desktop => Layer::Desktop,
            StatsLayer::Normal => Layer::Normal,
            StatsLayer::Top => Layer::Top,
        }
    }

    /// Moves a reading `delta` places along the order, stopping at the ends.
    pub fn move_reading(&mut self, r: StatReading, delta: i32) {
        let Some(from) = self.order.iter().position(|&x| x == r) else {
            return;
        };
        let to = (from as i32 + delta).clamp(0, self.order.len() as i32 - 1) as usize;
        let item = self.order.remove(from);
        self.order.insert(to, item);
    }

    /// Stores where each listed reading (part id, left, top) is now, and
    /// switches to the free structure so it stays there.
    pub fn seed_free(&mut self, spots: &[(u8, f32, f32)]) {
        for &(id, left, top) in spots {
            if let Some(&r) = StatReading::ALL.get(usize::from(id)) {
                let reading = self.reading_mut(r);
                (reading.x, reading.y) = (left, top);
            }
        }
        self.structure = StatsStructure::Free;
    }
}

/// Drops repeats, keeping the first of each, then appends what is missing in
/// the standard order, so a hand-edited `order` can neither lose nor double a
/// reading.
pub fn repair_order(order: &mut Vec<StatReading>) {
    let mut seen = Vec::with_capacity(StatReading::ALL.len());
    order.retain(|&r| {
        let fresh = !seen.contains(&r);
        seen.push(r);
        fresh
    });
    for r in StatReading::ALL {
        if !order.contains(&r) {
            order.push(r);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayerBackground {
    /// The theme's card.
    Card,
    /// The album art, blurred and darkened.
    #[default]
    ArtBlur,
    None,
}

/// The separately placed elements of the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerPart {
    Art,
    Title,
    Artist,
    Previous,
    Play,
    Next,
    /// The progress bar.
    Bar,
    /// Time played so far.
    Elapsed,
    /// Length of the track.
    Total,
}

impl PlayerPart {
    pub const ALL: [PlayerPart; 9] = [
        PlayerPart::Art,
        PlayerPart::Title,
        PlayerPart::Artist,
        PlayerPart::Previous,
        PlayerPart::Play,
        PlayerPart::Next,
        PlayerPart::Bar,
        PlayerPart::Elapsed,
        PlayerPart::Total,
    ];
}

/// A ready-made placement of every player element, as a starting point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerLayout {
    /// Art on the left, text and controls beside it.
    Row,
    /// Art, title, artist, controls and progress stacked on the centre line.
    Centered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerCfg {
    pub enabled: bool,
    pub opacity: f32,
    pub background: PlayerBackground,
    pub show_art: bool,
    pub show_progress: bool,
    pub show_controls: bool,
    /// Side of the album art square.
    pub art_size: f32,
    /// Corner rounding of the album art, as a fraction of its side. 0.5 is a circle.
    pub art_radius: f32,
    /// Length and thickness of the progress bar.
    pub bar_width: f32,
    pub bar_height: f32,
    /// Colour of the played part: `"text"`, `"accent"`, or a hex colour.
    pub bar_color: String,
    /// How visible the unplayed part of the bar is.
    pub bar_track_opacity: f32,
    /// Size of the previous, play and next buttons.
    pub button_size: f32,
    pub button_color: String,
    /// The most room the artist takes before being cut short. The title is
    /// never cut.
    pub text_width: f32,
    /// Show the name of the song alone, without the version, credits or
    /// film Spotify appends to it. See `lrclib::short_title`.
    pub short_title: bool,
    pub art: Spot,
    pub previous: Spot,
    pub play: Spot,
    pub next: Spot,
    pub bar: Spot,
    pub title: Label,
    pub artist: Label,
    pub elapsed: Label,
    pub total: Label,
    pub frame: Frame,
}

impl Default for PlayerCfg {
    fn default() -> Self {
        let mut cfg = Self {
            enabled: true,
            // Sits just above the full-width visualizer.
            frame: Frame::new(Anchor::BottomLeft, 56, 196, 420, 132),
            opacity: 1.0,
            background: PlayerBackground::ArtBlur,
            show_art: true,
            show_progress: true,
            show_controls: true,
            art_size: 0.0,
            art_radius: 0.14,
            bar_width: 0.0,
            bar_height: 4.0,
            bar_color: "accent".to_owned(),
            bar_track_opacity: 0.18,
            button_size: 30.0,
            button_color: "text".to_owned(),
            text_width: 270.0,
            short_title: true,
            art: Spot::default(),
            previous: Spot::default(),
            play: Spot::default(),
            next: Spot::default(),
            bar: Spot::default(),
            title: Label::sized(18.0, 600),
            artist: Label {
                opacity: 0.66,
                ..Label::sized(14.0, 400)
            },
            elapsed: Label {
                opacity: 0.66,
                ..Label::sized(12.0, 400)
            },
            total: Label {
                opacity: 0.66,
                ..Label::sized(12.0, 400)
            },
        };
        cfg.arrange(PlayerLayout::Row);
        cfg
    }
}

impl PlayerCfg {
    /// The position of one element, whichever kind it is.
    pub fn position_mut(&mut self, part: PlayerPart) -> (&mut f32, &mut f32) {
        fn spot(s: &mut Spot) -> (&mut f32, &mut f32) {
            (&mut s.x, &mut s.y)
        }
        fn label(l: &mut Label) -> (&mut f32, &mut f32) {
            (&mut l.x, &mut l.y)
        }
        match part {
            PlayerPart::Art => spot(&mut self.art),
            PlayerPart::Previous => spot(&mut self.previous),
            PlayerPart::Play => spot(&mut self.play),
            PlayerPart::Next => spot(&mut self.next),
            PlayerPart::Bar => spot(&mut self.bar),
            PlayerPart::Title => label(&mut self.title),
            PlayerPart::Artist => label(&mut self.artist),
            PlayerPart::Elapsed => label(&mut self.elapsed),
            PlayerPart::Total => label(&mut self.total),
        }
    }

    /// Places every element for one of the ready-made layouts. Only positions
    /// and the art and bar sizes change; fonts and colours are left alone.
    pub fn arrange(&mut self, layout: PlayerLayout) {
        let put = |label: &mut Label, x: f32, y: f32, align: Align| {
            (label.x, label.y, label.align) = (x, y, align);
        };
        match layout {
            PlayerLayout::Row => {
                (self.frame.w, self.frame.h) = (420, 132);
                self.art_size = 104.0;
                self.bar_width = 274.0;
                self.art = Spot::at(14.0, 14.0);
                put(&mut self.title, 132.0, 11.0, Align::Left);
                put(&mut self.artist, 132.0, 36.0, Align::Left);
                self.previous = Spot::at(126.0, 60.0);
                self.play = Spot::at(160.0, 60.0);
                self.next = Spot::at(194.0, 60.0);
                put(&mut self.elapsed, 132.0, 94.0, Align::Left);
                put(&mut self.total, 406.0, 94.0, Align::Right);
                self.bar = Spot::at(132.0, 114.0);
            }
            PlayerLayout::Centered => {
                (self.frame.w, self.frame.h) = (260, 400);
                self.art_size = 232.0;
                self.bar_width = 232.0;
                self.art = Spot::at(14.0, 14.0);
                put(&mut self.title, 130.0, 256.0, Align::Center);
                put(&mut self.artist, 130.0, 281.0, Align::Center);
                self.previous = Spot::at(81.0, 308.0);
                self.play = Spot::at(115.0, 308.0);
                self.next = Spot::at(149.0, 308.0);
                self.bar = Spot::at(14.0, 352.0);
                put(&mut self.elapsed, 14.0, 362.0, Align::Left);
                put(&mut self.total, 246.0, 362.0, Align::Right);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsMode {
    /// What is being sung lights up all at once. Files from before 1.2
    /// call it `line`.
    #[default]
    #[serde(alias = "line")]
    Complete,
    /// What is being sung fills from its start to its end. Files from
    /// before 1.2 call it `word`, which filled each word.
    #[serde(alias = "word")]
    Progress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    pub mode: LyricsMode,
    pub align: Align,
    /// Empty, or a font that is not available, uses the theme font.
    pub font: String,
    pub size: f32,
    /// Font weight, 100 to 900.
    pub weight: u32,
    /// Lines shown above and below the current one.
    pub lines_before: u32,
    pub lines_after: u32,
    /// Space between lines, as a fraction of the text size.
    pub line_gap: f32,
    /// Colour of the line being sung. Every lyric colour is `"text"`,
    /// `"accent"`, or a hex colour.
    pub active_color: String,
    /// Colour of the word being sung, in word mode.
    pub word_color: String,
    /// Colour of every other line, before `inactive_opacity` is applied.
    pub inactive_color: String,
    pub inactive_opacity: f32,
    /// Width of the outline around the text. 0 draws none.
    pub stroke_width: f32,
    pub stroke_color: String,
    pub shadow_color: String,
    /// How strong the shadow under the text is. 0 draws none.
    pub shadow_opacity: f32,
    /// Positive shows lyrics earlier.
    pub offset_ms: i32,
    /// Duration of the scroll between lines. 0 disables it.
    pub scroll_ms: u32,
    /// How much larger the line at the centre is drawn. 1 keeps every line
    /// the same size.
    pub active_scale: f32,
    /// How much each further line away from the centre fades, 0 to 0.9.
    pub falloff: f32,
    /// In word mode, the opacity of the words of the current line that have
    /// not been sung yet.
    pub unsung_opacity: f32,
    /// Follow the song word by word where the time each word is sung is
    /// known, which the servers below are asked for. Where it is not, and
    /// when this is off, the song is followed line by line: LRCLIB only
    /// knows when lines start, and a word's time is never guessed.
    pub word_sync: bool,
    /// LyricsPlus servers, tried in order.
    pub word_servers: Vec<String>,
    pub frame: Frame,
}

impl Default for LyricsCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::Right, 72, -80, 620, 370),
            card: false,
            opacity: 1.0,
            mode: LyricsMode::Complete,
            font: String::new(),
            align: Align::Right,
            size: 30.0,
            weight: 700,
            lines_before: 2,
            lines_after: 2,
            line_gap: 0.55,
            active_color: "text".to_owned(),
            word_color: "accent".to_owned(),
            inactive_color: "text".to_owned(),
            inactive_opacity: 0.38,
            stroke_width: 0.0,
            stroke_color: "#000000".to_owned(),
            shadow_color: "#000000".to_owned(),
            shadow_opacity: 0.38,
            offset_ms: 0,
            scroll_ms: 380,
            active_scale: 1.12,
            falloff: 0.25,
            unsung_opacity: 0.5,
            // Off by default: word timing comes from community-run mirrors of another service's lyrics (see the README). LRCLIB's line timing needs no opt-in.
            word_sync: false,
            word_servers: crate::lyricsplus::SERVERS.map(str::to_owned).to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisualizerStyle {
    /// Bars rising from the bottom edge.
    #[default]
    Bars,
    /// Bars growing both ways from the centre line.
    Mirror,
    /// A filled curve.
    Wave,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisualizerColor {
    /// The theme accent, fading out toward the base.
    #[default]
    Accent,
    /// `color_a` only.
    Solid,
    /// `color_a` at the bass end to `color_b` at the treble end.
    Gradient,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioSource {
    /// Only Spotify's audio.
    #[default]
    Spotify,
    /// Everything the PC plays.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualizerCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    pub style: VisualizerStyle,
    pub source: AudioSource,
    pub bars: u32,
    /// How wide each bar is, and the space between two, in
    /// display-independent pixels. Neither changes with the number of bars:
    /// bars that do not fit the widget are left out.
    pub bar_width: f32,
    pub bar_gap: f32,
    /// Draw a bar with no sound in it as a dot. Off, it is not drawn.
    pub show_idle: bool,
    /// Quarter turns clockwise, in degrees: 0, 90, 180 or 270. At 90 or
    /// 270 the bars run down the screen instead of across it.
    pub rotation: u32,
    /// Corner radius as a fraction of the bar width.
    pub radius: f32,
    /// Bass in the middle, treble at both edges.
    pub symmetric: bool,
    /// Treble on the left instead of the right.
    pub flip_x: bool,
    /// Bars hang from the top edge instead of standing on the bottom.
    pub flip_y: bool,
    /// Span the whole width of the screen, ignoring the frame's width and
    /// horizontal offset.
    pub full_width: bool,
    pub color: VisualizerColor,
    pub color_a: String,
    pub color_b: String,
    /// Gain in dB. Raise it if the bars barely move.
    pub sensitivity: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub fps: u32,
    pub min_hz: f32,
    pub max_hz: f32,
    /// Treble boost in dB per octave.
    pub tilt: f32,
    pub frame: Frame,
}

impl Default for VisualizerCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::Bottom, 0, 0, 760, 170),
            card: false,
            opacity: 0.9,
            style: VisualizerStyle::Bars,
            source: AudioSource::Spotify,
            bars: 120,
            bar_width: 10.0,
            bar_gap: 6.0,
            show_idle: true,
            rotation: 0,
            radius: 0.5,
            symmetric: false,
            flip_x: false,
            flip_y: false,
            full_width: true,
            color: VisualizerColor::Accent,
            color_a: "#7C5CFF".to_owned(),
            color_b: "#21D4FD".to_owned(),
            sensitivity: 0.0,
            attack_ms: 28.0,
            decay_ms: 190.0,
            fps: 60,
            min_hz: 40.0,
            max_hz: 15_000.0,
            tilt: 3.5,
        }
    }
}

/// More bars than this are thinner than a pixel on any current display.
pub const MAX_BARS: u32 = 512;

impl VisualizerCfg {
    /// Where the visualizer goes on a screen `area_w` wide: its own frame,
    /// or that frame stretched edge to edge when `full_width` is set.
    pub fn placed(&self, area_w: i32) -> Frame {
        if !self.full_width {
            return self.frame;
        }
        use Anchor::*;
        Frame {
            anchor: match self.frame.anchor {
                TopLeft | Top | TopRight => TopLeft,
                Left | Center | Right => Left,
                BottomLeft | Bottom | BottomRight => BottomLeft,
            },
            x: 0,
            w: area_w.max(1) as u32,
            ..self.frame
        }
    }
}

/// Lays `over` on top of `base`, table by table.
fn overlay(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(below)), toml::Value::Table(above)) => overlay(below, above),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Settings that used to be one key per widget and are now a key of each
/// element: section, old key, element, new key. One old key can feed two
/// elements.
const MOVED_KEYS: [(&str, &str, &str, &str); 9] = [
    ("clock", "day_font", "day", "font"),
    ("clock", "day_size", "day", "size"),
    ("clock", "time_font", "time", "font"),
    ("clock", "time_font", "date", "font"),
    ("clock", "time_size", "time", "size"),
    ("clock", "time_weight", "time", "weight"),
    ("clock", "text_size", "date", "size"),
    ("player", "title_size", "title", "size"),
    ("player", "artist_size", "artist", "size"),
];

/// Copies each old key to where it lives now, unless the new key is already
/// set, so a file written by an earlier version keeps its sizes and fonts.
fn carry_over_old_keys(user: &mut toml::Table) {
    for (section, old, element, key) in MOVED_KEYS {
        let Some(toml::Value::Table(section)) = user.get_mut(section) else {
            continue;
        };
        let Some(value) = section.get(old).cloned() else {
            continue;
        };
        let element = section
            .entry(element)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let toml::Value::Table(element) = element
            && !element.contains_key(key)
        {
            element.insert(key.to_owned(), value);
        }
    }
}

/// One-click looks. A preset only touches appearance, never positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Glass,
    Minimal,
    Neon,
    Mono,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Glass, Preset::Minimal, Preset::Neon, Preset::Mono];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Glass => "Glass",
            Preset::Minimal => "Minimal",
            Preset::Neon => "Neon",
            Preset::Mono => "Mono",
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let mut user: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        carry_over_old_keys(&mut user);
        // The file is laid over the defaults key by key. A table that sets
        // only some of an element's keys then keeps that element's own
        // defaults for the rest, rather than a generic blank.
        let mut merged = toml::Table::try_from(Config::default()).map_err(|e| e.to_string())?;
        overlay(&mut merged, user);
        merged
            .try_into::<Config>()
            .map(Config::sanitized)
            .map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        // Serializing plain structs of scalars cannot fail.
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Clamps every number to a range the renderer can draw. The file is
    /// hand-editable, so a typo must not be able to hang or blank a widget.
    pub fn sanitized(mut self) -> Self {
        fn unit(v: &mut f32) {
            *v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                1.0
            };
        }
        fn range(v: &mut f32, lo: f32, hi: f32, fallback: f32) {
            *v = if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            };
        }
        fn frame(f: &mut Frame) {
            f.w = f.w.clamp(80, 7680);
            f.h = f.h.clamp(40, 4320);
            f.x = f.x.clamp(-7680, 7680);
            f.y = f.y.clamp(-4320, 4320);
        }

        let g = &mut self.general;
        g.layout_height = g.layout_height.min(8640);

        let t = &mut self.theme;
        unit(&mut t.card_opacity);
        unit(&mut t.card_border);
        range(&mut t.card_radius, 0.0, 80.0, 22.0);

        let c = &mut self.clock;
        frame(&mut c.frame);
        unit(&mut c.opacity);
        for row in ClockRow::ALL {
            c.label_mut(row).sanitize();
        }

        let p = &mut self.player;
        frame(&mut p.frame);
        unit(&mut p.opacity);
        unit(&mut p.bar_track_opacity);
        range(&mut p.art_size, 0.0, 1000.0, 104.0);
        range(&mut p.art_radius, 0.0, 0.5, 0.14);
        range(&mut p.bar_width, 0.0, 4000.0, 274.0);
        range(&mut p.bar_height, 1.0, 60.0, 4.0);
        range(&mut p.button_size, 10.0, 200.0, 30.0);
        range(&mut p.text_width, 60.0, 4000.0, 270.0);
        for part in PlayerPart::ALL {
            let (x, y) = p.position_mut(part);
            sanitize_position(x, y);
        }
        for label in [&mut p.title, &mut p.artist, &mut p.elapsed, &mut p.total] {
            label.sanitize();
        }

        let l = &mut self.lyrics;
        frame(&mut l.frame);
        unit(&mut l.opacity);
        unit(&mut l.inactive_opacity);
        unit(&mut l.shadow_opacity);
        unit(&mut l.unsung_opacity);
        range(&mut l.active_scale, 1.0, 1.6, 1.12);
        range(&mut l.falloff, 0.0, 0.9, 0.25);
        range(&mut l.stroke_width, 0.0, 6.0, 0.0);
        range(&mut l.size, 10.0, 160.0, 30.0);
        range(&mut l.line_gap, 0.0, 3.0, 0.55);
        l.weight = l.weight.clamp(100, 900);
        l.lines_before = l.lines_before.min(8);
        l.lines_after = l.lines_after.min(8);
        l.offset_ms = l.offset_ms.clamp(-10_000, 10_000);
        l.scroll_ms = l.scroll_ms.min(2000);
        // Lyrics lookups carry the track's title and artist: never over plain HTTP.
        l.word_servers
            .retain(|server| server.starts_with("https://"));

        let v = &mut self.visualizer;
        frame(&mut v.frame);
        unit(&mut v.opacity);
        range(&mut v.bar_width, 1.0, 200.0, 10.0);
        range(&mut v.bar_gap, 0.0, 200.0, 6.0);
        v.rotation = v.rotation / 90 % 4 * 90;
        range(&mut v.radius, 0.0, 0.5, 0.5);
        range(&mut v.sensitivity, -30.0, 40.0, 0.0);
        range(&mut v.attack_ms, 0.0, 2000.0, 28.0);
        range(&mut v.decay_ms, 0.0, 5000.0, 190.0);
        range(&mut v.min_hz, 20.0, 2000.0, 40.0);
        range(&mut v.max_hz, 2000.0, 22_000.0, 15_000.0);
        range(&mut v.tilt, 0.0, 9.0, 3.5);
        v.bars = v.bars.clamp(4, MAX_BARS);
        v.fps = v.fps.clamp(10, 240);

        let s = &mut self.stats;
        frame(&mut s.frame);
        unit(&mut s.opacity);
        unit(&mut s.track_opacity);
        s.columns = s.columns.clamp(1, 9);
        s.refresh_ms = s.refresh_ms.clamp(500, 5000);
        range(&mut s.gap, 0.0, 200.0, 18.0);
        range(&mut s.padding, 0.0, 200.0, 14.0);
        range(&mut s.meter_length, 0.0, 2000.0, 0.0);
        range(&mut s.meter_thickness, 1.0, 60.0, 4.0);
        range(&mut s.ring_size, 12.0, 400.0, 28.0);
        s.name.sanitize();
        s.value.sanitize();
        repair_order(&mut s.order);
        for r in StatReading::ALL {
            let reading = s.reading_mut(r);
            range(&mut reading.warn, 0.0, 100.0, 0.0);
            if !(reading.max.is_finite() && (0.0..=1e12).contains(&reading.max)) {
                reading.max = 0.0;
            }
            sanitize_position(&mut reading.x, &mut reading.y);
            reading.name = reading.name.chars().take(24).collect();
        }

        self
    }

    pub fn apply_preset(&mut self, preset: Preset) {
        let defaults = Config::default();
        self.theme = Theme {
            font: std::mem::take(&mut self.theme.font),
            ..defaults.theme
        };
        self.clock.card = false;
        self.lyrics.card = false;
        self.visualizer.card = false;
        self.stats.card = false;
        self.player.background = PlayerBackground::ArtBlur;
        self.visualizer.color = VisualizerColor::Accent;
        self.visualizer.style = VisualizerStyle::Bars;
        self.lyrics.active_color = "text".to_owned();

        match preset {
            Preset::Glass => {
                self.lyrics.card = true;
                self.clock.card = true;
                self.stats.card = true;
            }
            Preset::Minimal => {
                self.player.background = PlayerBackground::None;
                self.theme.accent = "#FFFFFF".to_owned();
                self.visualizer.style = VisualizerStyle::Mirror;
            }
            Preset::Neon => {
                self.theme.card_color = "#0A0420".to_owned();
                self.theme.card_opacity = 0.78;
                self.theme.accent = "#FF3DCB".to_owned();
                self.visualizer.color = VisualizerColor::Gradient;
                self.visualizer.color_a = "#FF3DCB".to_owned();
                self.visualizer.color_b = "#21D4FD".to_owned();
                self.lyrics.active_color = "accent".to_owned();
                self.player.background = PlayerBackground::Card;
            }
            Preset::Mono => {
                self.theme.accent = "#FFFFFF".to_owned();
                self.theme.card_color = "#000000".to_owned();
                self.theme.card_radius = 6.0;
                self.player.background = PlayerBackground::Card;
                self.visualizer.color = VisualizerColor::Solid;
                self.visualizer.color_a = "#FFFFFF".to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_default_config() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn the_default_config_round_trips_through_toml() {
        let text = Config::default().to_toml();
        assert!(text.contains("[visualizer.frame]"), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), Config::default());
    }

    #[test]
    fn a_partial_file_keeps_defaults_and_ignores_unknown_keys() {
        let cfg = Config::from_toml(
            "[lyrics]\nmode = \"word\"\nfuture_key = 1\n\n[visualizer]\nstyle = \"wave\"\nbars = 24\n",
        )
        .unwrap();
        // The old name of the mode that fills.
        assert_eq!(cfg.lyrics.mode, LyricsMode::Progress);
        assert_eq!(cfg.lyrics.size, LyricsCfg::default().size);
        assert_eq!(cfg.visualizer.style, VisualizerStyle::Wave);
        assert_eq!(cfg.visualizer.bars, 24);
        assert_eq!(cfg.clock, ClockCfg::default());
    }

    #[test]
    fn word_servers_keep_only_https() {
        let cfg = Config::from_toml(
            "[lyrics]\nword_servers = [\"http://a.example\", \"https://b.example\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.lyrics.word_servers, ["https://b.example"]);
    }

    #[test]
    fn a_syntax_error_is_reported_not_swallowed() {
        assert!(Config::from_toml("[lyrics\nmode = ").is_err());
        assert!(Config::from_toml("[lyrics]\nmode = \"sideways\"").is_err());
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let cfg = Config::from_toml(
            "[visualizer]\nbars = 100000\nfps = 0\nbar_gap = 750.0\nrotation = 100\n[lyrics]\nsize = -4.0\n[clock.frame]\nw = 1\n",
        )
        .unwrap();
        assert_eq!(cfg.visualizer.bars, MAX_BARS);
        assert_eq!(cfg.visualizer.fps, 10);
        assert_eq!(cfg.visualizer.bar_gap, 200.0);
        assert_eq!(cfg.visualizer.rotation, 90);
        assert_eq!(cfg.lyrics.size, 10.0);
        assert_eq!(cfg.clock.frame.w, 80);
    }

    #[test]
    fn anchors_place_a_frame_from_the_right_edge() {
        let area = (1920, 1040);
        let at = |anchor| Frame::new(anchor, 50, 30, 400, 100).origin(area.0, area.1);
        assert_eq!(at(Anchor::TopLeft), (50, 30));
        assert_eq!(at(Anchor::TopRight), (1470, 30));
        assert_eq!(at(Anchor::BottomRight), (1470, 910));
        assert_eq!(at(Anchor::Center), (810, 500));
        assert_eq!(at(Anchor::Bottom), (810, 910));
    }

    #[test]
    fn a_drag_keeps_the_anchor_and_lands_where_dropped() {
        for anchor in [
            Anchor::TopLeft,
            Anchor::Top,
            Anchor::TopRight,
            Anchor::Left,
            Anchor::Center,
            Anchor::Right,
            Anchor::BottomLeft,
            Anchor::Bottom,
            Anchor::BottomRight,
        ] {
            let mut frame = Frame::new(anchor, 50, 30, 400, 100);
            frame.set_origin(123, 456, 1920, 1040);
            assert_eq!(frame.anchor, anchor);
            assert_eq!(frame.origin(1920, 1040), (123, 456), "{anchor:?}");
        }
    }

    #[test]
    fn reanchoring_keeps_the_place_and_follows_a_smaller_screen() {
        let big = (2560, 1400);
        let small = (1920, 1040);
        // Dragged to the middle while measured from the top-left corner.
        let mut middle = Frame::new(Anchor::TopLeft, 1080, 600, 400, 200);
        middle.reanchor(big.0, big.1);
        assert_eq!(middle.anchor, Anchor::Center);
        assert_eq!(middle.origin(big.0, big.1), (1080, 600));
        assert_eq!(middle.origin(small.0, small.1), (760, 420));

        let mut corner = Frame::new(Anchor::TopLeft, 2100, 1150, 400, 200);
        corner.reanchor(big.0, big.1);
        assert_eq!(corner.anchor, Anchor::BottomRight);
        assert_eq!(corner.origin(big.0, big.1), (2100, 1150));
        assert_eq!(corner.origin(small.0, small.1), (1460, 790));

        let mut top = Frame::new(Anchor::Center, 0, -500, 400, 100);
        top.reanchor(big.0, big.1);
        assert_eq!(top.anchor, Anchor::Top);
        assert_eq!(top.origin(big.0, big.1), (1080, 150));
    }

    #[test]
    fn a_frame_beyond_the_edge_is_pulled_back_on_screen() {
        let frame = Frame::new(Anchor::TopLeft, 1800, -40, 400, 100);
        assert_eq!(frame.origin_within(1920, 1040), (1520, 0));
        let huge = Frame::new(Anchor::Center, 0, 0, 3000, 2000);
        assert_eq!(huge.origin_within(1920, 1040), (0, 0));
    }

    #[test]
    fn widgets_grow_and_shrink_with_the_screen() {
        let mut general = General {
            layout_height: 1400,
            ..General::default()
        };
        assert_eq!(general.layout_scale(1400.0), 1.0);
        assert!((general.layout_scale(1050.0) - 0.75).abs() < 1e-6);
        assert_eq!(general.layout_scale(f32::NAN), 1.0);
        general.fit_screen = false;
        assert_eq!(general.layout_scale(1050.0), 1.0);
        general.fit_screen = true;
        general.layout_height = 0;
        assert_eq!(general.layout_scale(1050.0), 1.0);
    }

    #[test]
    fn the_default_layout_fits_a_small_screen_without_overlap() {
        // A 1080p laptop at 150% scaling, less the taskbar: the smallest
        // desktop this is likely to meet.
        let (w, h) = (1280, 672);
        let cfg = Config::default();
        let boxes: Vec<[i32; 4]> = [
            cfg.clock.frame,
            cfg.player.frame,
            cfg.lyrics.frame,
            cfg.visualizer.placed(w),
        ]
        .iter()
        .map(|frame| {
            let (x, y) = frame.origin(w, h);
            [x, y, x + frame.w as i32, y + frame.h as i32]
        })
        .collect();

        for (i, a) in boxes.iter().enumerate() {
            assert!(
                a[0] >= 0 && a[1] >= 0 && a[2] <= w && a[3] <= h,
                "{a:?} is off screen"
            );
            for b in &boxes[i + 1..] {
                let apart = a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1];
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn the_default_stats_frame_clears_the_clock_and_visualizer() {
        let (w, h) = (1280, 672);
        let cfg = Config::default();
        let rect = |frame: Frame| {
            let (x, y) = frame.origin(w, h);
            [x, y, x + frame.w as i32, y + frame.h as i32]
        };
        let a = rect(cfg.stats.frame);
        for b in [rect(cfg.clock.frame), rect(cfg.visualizer.placed(w))] {
            let apart = a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1];
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }

    #[test]
    fn the_stats_tables_are_written_and_a_partial_one_keeps_the_rest() {
        let text = Config::default().to_toml();
        assert!(text.contains("[stats.cpu]"), "{text}");
        assert!(text.contains("[stats.frame]"), "{text}");
        let cfg = Config::from_toml("[stats.cpu]\nname = \"Proc\"\n").unwrap();
        assert_eq!(cfg.stats.cpu.name, "Proc");
        assert_eq!(cfg.stats.cpu.warn, 90.0);
        assert_eq!(cfg.stats.ram, StatsCfg::default().ram);
    }

    #[test]
    fn stats_numbers_are_clamped() {
        let cfg =
            Config::from_toml("[stats]\ncolumns = 99\nrefresh_ms = 1\n[stats.cpu]\nwarn = 400.0\n")
                .unwrap();
        assert_eq!(cfg.stats.columns, 9);
        assert_eq!(cfg.stats.refresh_ms, 500);
        assert_eq!(cfg.stats.cpu.warn, 100.0);
    }

    #[test]
    fn a_stats_order_is_repaired() {
        use StatReading::*;
        let mut order = vec![Ram, Ram, Cpu];
        repair_order(&mut order);
        assert_eq!(order, [Ram, Cpu, Gpu, Vram, Disks, Down, Up, Battery]);
        let cfg = Config::from_toml("[stats]\norder = [\"up\"]\n").unwrap();
        assert_eq!(cfg.stats.order.len(), 8);
        assert_eq!(cfg.stats.order[0], Up);
    }

    #[test]
    fn readings_move_along_the_order_without_wrapping() {
        let mut s = StatsCfg::default();
        s.move_reading(StatReading::Cpu, -1);
        assert_eq!(s.order, StatReading::ALL);
        s.move_reading(StatReading::Cpu, 2);
        assert_eq!(s.order[2], StatReading::Cpu);
        s.move_reading(StatReading::Battery, 5);
        assert_eq!(s.order[7], StatReading::Battery);
    }

    #[test]
    fn seeding_the_free_structure_moves_only_the_listed_readings() {
        let mut s = StatsCfg::default();
        s.seed_free(&[(1, 5.0, 6.0)]);
        assert_eq!(s.structure, StatsStructure::Free);
        assert_eq!((s.ram.x, s.ram.y), (5.0, 6.0));
        assert_eq!(s.cpu, StatsCfg::default().cpu);
    }

    #[test]
    fn the_stats_layer_follows_the_general_one_unless_set() {
        let mut general = General {
            layer: Layer::Normal,
            ..General::default()
        };
        let mut s = StatsCfg::default();
        assert_eq!(s.resolved_layer(&general), Layer::Normal);
        general.layer = Layer::Desktop;
        s.layer = StatsLayer::Top;
        assert_eq!(s.resolved_layer(&general), Layer::Top);
    }

    #[test]
    fn reading_ids_are_their_positions() {
        for (i, r) in StatReading::ALL.iter().enumerate() {
            assert_eq!(*r as usize, i);
        }
    }

    #[test]
    fn a_full_width_visualizer_spans_the_screen_at_its_own_height() {
        let mut v = VisualizerCfg {
            frame: Frame::new(Anchor::Bottom, 40, 12, 300, 90),
            ..VisualizerCfg::default()
        };
        assert!(v.full_width);
        let placed = v.placed(1920);
        assert_eq!(placed.origin(1920, 1040), (0, 1040 - 90 - 12));
        assert_eq!((placed.w, placed.h), (1920, 90));

        v.full_width = false;
        assert_eq!(v.placed(1920), v.frame);
    }

    #[test]
    fn a_config_from_before_free_placement_still_loads() {
        let cfg = Config::from_toml(
            "[clock]\nalign = \"left\"\norder = [\"date\"]\n[player]\nlayout = \"centered\"\norder = [\"art\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.clock, ClockCfg::default());
        assert_eq!(cfg.player, PlayerCfg::default());
    }

    #[test]
    fn one_key_of_one_element_changes_nothing_else() {
        let cfg =
            Config::from_toml("[player.title]\nx = 300.0\n[clock.day]\nsize = 60.0\n").unwrap();
        let fresh = Config::default();
        assert_eq!(cfg.player.title.x, 300.0);
        // The rest of that element keeps its own defaults, not a blank's.
        assert_eq!(cfg.player.title.y, fresh.player.title.y);
        assert_eq!(cfg.player.title.size, fresh.player.title.size);
        assert_eq!(cfg.player.artist, fresh.player.artist);
        assert_eq!(cfg.player.art, fresh.player.art);
        assert_eq!(cfg.clock.day.size, 60.0);
        assert_eq!(cfg.clock.day.font, "Anurati");
        assert!(cfg.clock.day.uppercase);
    }

    #[test]
    fn a_label_reads_its_direction_and_letters() {
        let cfg = Config::from_toml(
            "[clock.day]\ndirection = \"down\"\n[player.title]\ndirection = \"up\"\nletters = \"sideways\"\n",
        )
        .unwrap();
        assert_eq!(cfg.clock.day.direction, Direction::Down);
        assert_eq!(cfg.clock.day.letters, Letters::Upright);
        assert_eq!(cfg.player.title.direction, Direction::Up);
        assert_eq!(cfg.player.title.letters, Letters::Sideways);
        assert_eq!(cfg.clock.time.direction, Direction::Horizontal);
    }

    #[test]
    fn the_centred_arrangement_moves_things_and_restyles_nothing() {
        let mut p = PlayerCfg::default();
        p.title.size = 31.0;
        p.arrange(PlayerLayout::Centered);
        assert_eq!(p.title.align, Align::Center);
        assert_eq!(p.title.x * 2.0, p.frame.w as f32);
        assert_eq!(p.art.x * 2.0 + p.art_size, p.frame.w as f32);
        assert_eq!(p.title.size, 31.0);

        p.title.size = PlayerCfg::default().title.size;
        p.arrange(PlayerLayout::Row);
        assert_eq!(p, PlayerCfg::default());
    }

    #[test]
    fn a_position_that_is_not_a_number_is_reset() {
        let cfg = Config::from_toml("[clock.day]\nx = nan\ny = 1e9\nsize = -3.0\n").unwrap();
        assert_eq!((cfg.clock.day.x, cfg.clock.day.y), (0.0, 8000.0));
        assert_eq!(cfg.clock.day.size, 6.0);
    }

    #[test]
    fn sizes_and_fonts_from_an_older_file_carry_over() {
        let cfg = Config::from_toml(
            "[clock]\nday_size = 50.0\ntime_font = \"Inter\"\ntime_weight = 700\n[player]\ntitle_size = 22.0\n",
        )
        .unwrap();
        assert_eq!(cfg.clock.day.size, 50.0);
        assert_eq!(cfg.clock.time.font, "Inter");
        assert_eq!(cfg.clock.date.font, "Inter");
        assert_eq!(cfg.clock.time.weight, 700);
        assert_eq!(cfg.player.title.size, 22.0);
        // Untouched by the old file.
        assert_eq!(cfg.clock.day.font, "Anurati");
    }

    #[test]
    fn a_new_key_beats_the_old_one_it_replaced() {
        let cfg =
            Config::from_toml("[clock]\nday_size = 50.0\n[clock.day]\nsize = 40.0\n").unwrap();
        assert_eq!(cfg.clock.day.size, 40.0);
    }

    #[test]
    fn the_bar_count_is_capped() {
        let cfg = Config::from_toml("[visualizer]\nbars = 100000\n").unwrap();
        assert_eq!(cfg.visualizer.bars, MAX_BARS);
    }

    #[test]
    fn presets_change_the_look_but_never_the_layout_or_font() {
        for preset in Preset::ALL {
            let mut cfg = Config::default();
            cfg.theme.font = "Inter".to_owned();
            cfg.lyrics.frame.x = 999;
            cfg.apply_preset(preset);
            assert_eq!(cfg.theme.font, "Inter", "{}", preset.name());
            assert_eq!(cfg.lyrics.frame.x, 999);
            assert_eq!(cfg.clone().sanitized(), cfg);
        }
    }
}
