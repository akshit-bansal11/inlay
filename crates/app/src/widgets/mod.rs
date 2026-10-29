//! The four widgets and what they share.

pub mod clock;
pub mod lyrics;
pub mod player;
pub mod visualizer;

use std::time::Duration;

use inlay_core::color::{Rgba, parse_hex, with_alpha};
use inlay_core::config::{Align, ClockRow, Config, Direction, Frame, Label, Letters, PlayerPart};
use inlay_core::timefmt::LocalTime;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;
use windows::core::Result;

use crate::capture::Audio;
use crate::gfx::{Gfx, TextStyle, rect};
use crate::lyrics::Lyrics;
use crate::media::MediaState;

/// When a widget next needs to run. The main loop sleeps for as long as the
/// most impatient widget allows, and forever if every widget is `Idle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wake {
    /// Nothing will change until an event arrives.
    Idle,
    After(Duration),
    /// Something is animating: run again on the next display refresh.
    Frame,
}

impl Wake {
    pub fn sooner(self, other: Wake) -> Wake {
        match (self, other) {
            (Wake::Frame, _) | (_, Wake::Frame) => Wake::Frame,
            (Wake::After(a), Wake::After(b)) => Wake::After(a.min(b)),
            (Wake::After(a), Wake::Idle) | (Wake::Idle, Wake::After(a)) => Wake::After(a),
            (Wake::Idle, Wake::Idle) => Wake::Idle,
        }
    }

    pub fn after_ms(ms: f64) -> Wake {
        Wake::After(Duration::from_secs_f64(ms.max(1.0) / 1000.0))
    }
}

pub struct Tick {
    /// The widget looks different from what is on screen.
    pub dirty: bool,
    pub wake: Wake,
}

/// Everything a widget may read. Widgets own no app state.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub media: &'a MediaState,
    pub lyrics: &'a Lyrics,
    /// Bumped whenever `lyrics` is replaced.
    pub lyrics_gen: u64,
    pub audio: &'a Audio,
    /// Milliseconds on the app's monotonic clock.
    pub now_ms: f64,
    pub time: LocalTime,
    pub ms_to_next_second: u32,
    pub text: Rgba,
    pub card: Rgba,
    /// The theme accent, already resolved from the album art when set to auto.
    pub accent: Rgba,
}

pub enum Action {
    PlayPause,
    Next,
    Previous,
    /// Seek to this fraction of the track.
    Seek(f64),
}

pub trait Widget {
    /// Advances the widget's state. Cheap, and called far more often than `draw`.
    fn tick(&mut self, ctx: &Ctx) -> Tick;

    /// Draws into a `w` by `h` box whose top-left corner is the origin.
    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()>;

    /// Drops everything derived from the config or the graphics device.
    fn reset(&mut self) {}

    /// The separately placed elements drawn last time. In edit mode each
    /// can be dragged on its own.
    fn parts(&self) -> &[Part] {
        &[]
    }

    /// The pointer moved to `at`, or left. Returns whether to redraw.
    fn hover(&mut self, _at: Option<(f32, f32)>) -> bool {
        false
    }

    fn click(&mut self, _x: f32, _y: f32) -> Option<Action> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Clock,
    Player,
    Lyrics,
    Visualizer,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Visualizer, Kind::Lyrics, Kind::Clock, Kind::Player];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Clock => "Clock",
            Kind::Player => "Player",
            Kind::Lyrics => "Lyrics",
            Kind::Visualizer => "Visualizer",
        }
    }

    pub fn create(self) -> Box<dyn Widget> {
        match self {
            Kind::Clock => Box::new(clock::Clock::default()),
            Kind::Player => Box::new(player::Player::default()),
            Kind::Lyrics => Box::new(lyrics::LyricsView::default()),
            Kind::Visualizer => Box::new(visualizer::Visualizer::new()),
        }
    }

    /// Where the widget goes on a screen `area_w` wide.
    pub fn placed(self, cfg: &Config, area_w: i32) -> Frame {
        match self {
            Kind::Clock => cfg.clock.frame,
            Kind::Player => cfg.player.frame,
            Kind::Lyrics => cfg.lyrics.frame,
            Kind::Visualizer => cfg.visualizer.placed(area_w),
        }
    }

    pub fn frame_mut(self, cfg: &mut Config) -> &mut Frame {
        match self {
            Kind::Clock => &mut cfg.clock.frame,
            Kind::Player => &mut cfg.player.frame,
            Kind::Lyrics => &mut cfg.lyrics.frame,
            Kind::Visualizer => &mut cfg.visualizer.frame,
        }
    }

    pub fn enabled(self, cfg: &Config) -> bool {
        match self {
            Kind::Clock => cfg.clock.enabled,
            Kind::Player => cfg.player.enabled,
            Kind::Lyrics => cfg.lyrics.enabled,
            Kind::Visualizer => cfg.visualizer.enabled,
        }
    }

    /// Moves one of the widget's own elements, by the id it reported in
    /// `Widget::parts`.
    pub fn nudge(self, cfg: &mut Config, id: u8, dx: f32, dy: f32) {
        let position = match self {
            Kind::Clock => ClockRow::ALL.get(id as usize).map(|&row| {
                let label = cfg.clock.label_mut(row);
                (&mut label.x, &mut label.y)
            }),
            Kind::Player => PlayerPart::ALL
                .get(id as usize)
                .map(|&part| cfg.player.position_mut(part)),
            Kind::Lyrics | Kind::Visualizer => None,
        };
        if let Some((x, y)) = position {
            *x += dx;
            *y += dy;
        }
    }

    /// Widgets with no box of their own: their window is kept wrapped around
    /// wherever their elements have been put.
    pub fn fitted(self) -> bool {
        matches!(self, Kind::Clock | Kind::Player)
    }

    /// How many separately placed elements the widget has.
    pub fn part_count(self) -> u8 {
        match self {
            Kind::Clock => ClockRow::ALL.len() as u8,
            Kind::Player => PlayerPart::ALL.len() as u8,
            Kind::Lyrics | Kind::Visualizer => 0,
        }
    }

    /// Widgets with nothing to show while Spotify is closed.
    pub fn needs_spotify(self) -> bool {
        self != Kind::Clock
    }

    /// Widgets that take clicks; the rest are click-through.
    pub fn interactive(self) -> bool {
        self == Kind::Player
    }
}

/// The theme's translucent rounded card, filling the widget.
pub fn draw_card(g: &Gfx, w: f32, h: f32, ctx: &Ctx, opacity: f32) {
    let theme = &ctx.cfg.theme;
    let radius = theme.card_radius.min(w.min(h) / 2.0);
    g.fill_round(
        rect(0.0, 0.0, w, h),
        radius,
        with_alpha(ctx.card, theme.card_opacity * opacity),
    );
    if theme.card_border > 0.0 {
        g.stroke_round(
            rect(0.5, 0.5, w - 1.0, h - 1.0),
            radius,
            [1.0, 1.0, 1.0, theme.card_border * opacity],
            1.0,
        );
    }
}

/// An element's id within its widget, and the box it was drawn in.
pub type Part = (u8, D2D_RECT_F);

pub fn contains(r: &D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

/// A line of text laid out where its label puts it.
pub struct Placed {
    layout: IDWriteTextLayout,
    /// Quarter turns clockwise to draw the layout at, then how far to move it.
    quarter: i8,
    offset: (f32, f32),
    /// The box the text itself covers.
    pub rect: D2D_RECT_F,
}

/// Draws a placed label where, and whichever way up, it was laid out.
pub fn draw_placed(g: &Gfx, placed: &Placed, color: Rgba, shadow: bool) {
    let old = g.turn(placed.quarter, placed.offset.0, placed.offset.1);
    g.draw_text(&placed.layout, 0.0, 0.0, color, shadow);
    g.restore(old);
}

/// The box `r` covers after `quarter` quarter turns clockwise about the origin.
fn turned(r: D2D_RECT_F, quarter: i8) -> D2D_RECT_F {
    match quarter {
        1 => D2D_RECT_F {
            left: -r.bottom,
            top: r.left,
            right: -r.top,
            bottom: r.right,
        },
        -1 => D2D_RECT_F {
            left: r.top,
            top: -r.right,
            right: r.bottom,
            bottom: -r.left,
        },
        _ => r,
    }
}

/// Share of a line box, above and below, that holds no ink.
const LINE_BOX_TRIM: f32 = 0.15;
/// The narrowest a line is ever squeezed to before it is cut with an ellipsis.
const MIN_TEXT_WIDTH: f32 = 60.0;

/// A colour setting: the theme's text or accent colour by name, or a hex colour.
pub fn named_color(name: &str, ctx: &Ctx) -> Rgba {
    match name {
        "accent" => ctx.accent,
        "text" | "" => ctx.text,
        hex => parse_hex(hex).unwrap_or(ctx.text),
    }
}

/// The colour a label draws in, inside a widget drawn at `opacity`.
pub fn label_color(label: &Label, ctx: &Ctx, opacity: f32) -> Rgba {
    with_alpha(named_color(&label.color, ctx), label.opacity * opacity)
}

/// Lays out one piece of text the way its label asks: font, size, weight,
/// letter spacing, capitals, which way it runs, and which edge sits on its
/// position. The text takes at most `room` and is cut with an ellipsis beyond.
pub fn place_label(g: &mut Gfx, text: &str, label: &Label, ctx: &Ctx, room: f32) -> Result<Placed> {
    let room = room.max(MIN_TEXT_WIDTH);
    // A font that is missing would be substituted by something unrelated;
    // the theme font is the better stand-in.
    let font = if !label.font.is_empty() && g.has_font(&label.font) {
        label.font.as_str()
    } else {
        ctx.cfg.theme.font.as_str()
    };
    // Every way a line can run is one of two layouts, the line itself or its
    // letters stacked one under another, drawn as it is or a quarter turned.
    let (stack, quarter) = match (label.direction, label.letters) {
        (Direction::Horizontal, Letters::Upright) => (false, 0),
        (Direction::Down, Letters::Sideways) => (false, 1),
        (Direction::Up, Letters::Sideways) => (false, -1),
        (Direction::Down | Direction::Up, Letters::Upright) => (true, 0),
        (Direction::Horizontal, Letters::Sideways) => (true, -1),
    };
    let mut text = if label.uppercase {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    if stack {
        // ponytail: one char per line, so an accent typed as its own code
        // point gets a line to itself. Split by grapheme if that ever shows.
        let mut letters: Vec<String> = text.chars().map(String::from).collect();
        if label.direction == Direction::Up {
            letters.reverse();
        }
        text = letters.join("\n");
    }
    let align = if stack { Align::Center } else { label.align };
    let style = TextStyle {
        font,
        size: label.size,
        weight: label.weight,
        align,
        wrap: false,
    };
    let layout = g.layout(&text, &style, room, 10_000.0)?;
    if label.spacing > 0.0 && !stack {
        let units = text.encode_utf16().count() as u32;
        Gfx::letter_space(&layout, units, label.spacing * label.size)?;
    }
    let (text_w, text_h) = Gfx::measure(&layout);
    let text_w = text_w.min(room);
    // Where the text begins inside its layout box, which is `room` wide.
    let lead = match align {
        Align::Left => 0.0,
        Align::Right => room - text_w,
        Align::Center => (room - text_w) / 2.0,
    };
    // The label's position marks whichever edge of the text it grows from.
    // A line that runs across grows sideways, so the position is its top
    // and its left, centre or right. One that runs down or up grows along
    // its length, so the same choice is its top, middle or bottom too:
    // "Wednesday" then stays centred where "Tuesday" was.
    let drawn = turned(rect(lead, 0.0, text_w, text_h), quarter);
    let (width, height) = (drawn.right - drawn.left, drawn.bottom - drawn.top);
    let left = match label.align {
        Align::Left => label.x,
        Align::Right => label.x - width,
        Align::Center => label.x - width / 2.0,
    };
    let top = match (label.direction, label.align) {
        (Direction::Horizontal, _) | (_, Align::Left) => label.y,
        (_, Align::Center) => label.y - height / 2.0,
        (_, Align::Right) => label.y - height,
    };
    let offset = (left - drawn.left, top - drawn.top);
    // A line box is taller than its letters, most of all at large sizes.
    // Trimmed, so the grab box of one row does not cover its neighbours.
    let trim = if stack { 0.0 } else { text_h * LINE_BOX_TRIM };
    let grab = turned(rect(lead, trim, text_w, text_h - 2.0 * trim), quarter);
    Ok(Placed {
        layout,
        quarter,
        offset,
        rect: D2D_RECT_F {
            left: grab.left + offset.0,
            top: grab.top + offset.1,
            right: grab.right + offset.0,
            bottom: grab.bottom + offset.1,
        },
    })
}

/// `m:ss`, for track positions.
pub fn clock_text(ms: f64) -> String {
    let seconds = (ms.max(0.0) / 1000.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_impatient_wake_wins() {
        let short = Wake::After(Duration::from_millis(10));
        let long = Wake::After(Duration::from_millis(900));
        assert_eq!(short.sooner(long), short);
        assert_eq!(Wake::Idle.sooner(long), long);
        assert_eq!(long.sooner(Wake::Frame), Wake::Frame);
        assert_eq!(Wake::Idle.sooner(Wake::Idle), Wake::Idle);
    }

    #[test]
    fn a_quarter_turn_lays_a_box_on_its_side() {
        let line = rect(10.0, 0.0, 100.0, 20.0);
        // Clockwise: the line runs down, to the left of the origin.
        let down = turned(line, 1);
        assert_eq!(
            (down.left, down.top, down.right, down.bottom),
            (-20.0, 10.0, 0.0, 110.0)
        );
        // Anticlockwise: it runs up, above the origin.
        let up = turned(line, -1);
        assert_eq!(
            (up.left, up.top, up.right, up.bottom),
            (0.0, -110.0, 20.0, -10.0)
        );
        assert_eq!(turned(line, 0), line);
    }

    #[test]
    fn formats_track_positions() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(83_900.0), "1:23");
        assert_eq!(clock_text(-5.0), "0:00");
        assert_eq!(clock_text(3_600_000.0), "60:00");
    }
}
