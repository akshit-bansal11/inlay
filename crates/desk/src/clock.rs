//! The clock widget's drawing.
use std::time::Duration;

use chrono::{Datelike, Timelike};
use eframe::egui::{Align2, Color32, FontId, Painter, Rect, vec2};
use inlay_core::color::parse_hex;
use inlay_core::config::{Align, ClockCfg, ClockRow, Config, Theme};
use inlay_core::timefmt::{LocalTime, format, shows_seconds};

/// The wall-clock time in the local zone.
pub fn local_now() -> LocalTime {
    let n = chrono::Local::now();
    LocalTime {
        year: n.year().max(0) as u32,
        month: n.month(),
        day: n.day(),
        weekday: n.weekday().num_days_from_sunday(),
        hour: n.hour(),
        minute: n.minute(),
        second: n.second(),
    }
}

/// A config colour name (`text`, `accent` or a hex string) as a colour; white if unparseable.
pub fn color_of(name: &str, theme: &Theme) -> Color32 {
    let hex = match name {
        "text" => &theme.text,
        "accent" => &theme.accent,
        other => other,
    };
    let Some([r, g, b, a]) = parse_hex(hex) else {
        return Color32::WHITE;
    };
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(a))
}

/// Draws the day, time and date rows at their configured spots inside the widget's rect.
pub fn draw(painter: &Painter, rect: Rect, cfg: &Config, now: &LocalTime, scale: f32) {
    let clock = &cfg.clock;
    for row in ClockRow::ALL {
        let (shown, fmt) = match row {
            ClockRow::Day => (clock.show_day, &clock.day_format),
            ClockRow::Time => (clock.show_time, &clock.time_format),
            ClockRow::Date => (clock.show_date, &clock.date_format),
        };
        if !shown {
            continue;
        }
        let label = clock.label(row);
        let mut text = format(fmt, now);
        if label.uppercase {
            text = text.to_uppercase();
        }
        let anchor = match label.align {
            Align::Left => Align2::LEFT_TOP,
            Align::Center => Align2::CENTER_TOP,
            Align::Right => Align2::RIGHT_TOP,
        };
        let color =
            color_of(&label.color, &cfg.theme).gamma_multiply(label.opacity * clock.opacity);
        // ponytail: weight, letter spacing and vertical direction are not drawn yet.
        painter.text(
            rect.min + vec2(label.x, label.y) * scale,
            anchor,
            text,
            FontId::new(
                label.size * scale,
                crate::fonts::family(&label.font, &cfg.theme.font),
            ),
            color,
        );
    }
}

/// How long until the shown text can next change: the next second if a format shows seconds, else the next minute.
pub fn next_change(cfg: &ClockCfg, now: &LocalTime) -> Duration {
    let secs = [
        (cfg.show_day, &cfg.day_format),
        (cfg.show_time, &cfg.time_format),
        (cfg.show_date, &cfg.date_format),
    ]
    .iter()
    .any(|(shown, f)| *shown && shows_seconds(f));
    let ms = u64::from(chrono::Local::now().timestamp_subsec_millis());
    let left = if secs { 1 } else { 60 - now.second.min(59) };
    // Aligned to the tick, plus 50 ms so the repaint lands after the text changes, not before.
    Duration::from_millis((u64::from(left) * 1000).saturating_sub(ms) + 50)
}
