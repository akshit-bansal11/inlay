//! The player widget's drawing: art, title, artist, transport buttons and progress.
use eframe::egui::{
    Align2, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke,
    TextureHandle, Ui, epaint::RectShape, pos2, vec2,
};
use inlay_core::config::{Align, Config, Label};
use inlay_core::lrclib::short_title;

use crate::clock::color_of;
use crate::fonts::family;
use crate::model::{Command, Track};

/// Extra distance above and below the bar that still counts as on it, for clicking to seek.
const BAR_SLOP: f32 = 9.0;
/// Glyph size as a share of the button: the play glyph is drawn larger.
const GLYPH_SHARE: f32 = 0.47;
const PLAY_GLYPH_SHARE: f32 = 0.57;

/// A track time as `m:ss`.
fn clock_text(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The track position a click at `x` on a bar starting at `left`, `width` wide, asks for.
fn seek_ms(x: f32, left: f32, width: f32, duration_ms: i64) -> i64 {
    if width <= 0.0 {
        return 0;
    }
    let fraction = f64::from(((x - left) / width).clamp(0.0, 1.0));
    (fraction * duration_ms as f64).round() as i64
}

/// Decodes album art bytes (PNG or JPEG) into an egui image, or None.
pub fn decode_art(bytes: &[u8]) -> Option<ColorImage> {
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// One text label at its spot, offset by `origin` and scaled.
fn text(ui: &Ui, origin: Pos2, label: &Label, text: &str, cfg: &Config, scale: f32) {
    let anchor = match label.align {
        Align::Left => Align2::LEFT_TOP,
        Align::Center => Align2::CENTER_TOP,
        Align::Right => Align2::RIGHT_TOP,
    };
    let text = if label.uppercase {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    let color =
        color_of(&label.color, &cfg.theme).gamma_multiply(label.opacity * cfg.player.opacity);
    // ponytail: weight beyond the loaded face, letter spacing and vertical direction are
    // not drawn; see fonts.rs.
    ui.painter().text(
        origin + vec2(label.x, label.y) * scale,
        anchor,
        text,
        FontId::new(label.size * scale, family(&label.font, &cfg.theme.font)),
        color,
    );
}

/// A transport glyph centred in `button`.
#[derive(Clone, Copy, PartialEq)]
enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
}

fn draw_glyph(ui: &Ui, glyph: Glyph, button: Rect, size: f32, color: Color32) {
    let c = button.center();
    let h = size / 2.0;
    // A right-pointing triangle centred on `cx`, mirrored when `dir` is -1.
    let triangle = |cx: f32, dir: f32| {
        Shape::convex_polygon(
            vec![
                pos2(cx - dir * h * 0.8, c.y - h),
                pos2(cx + dir * h * 0.8, c.y),
                pos2(cx - dir * h * 0.8, c.y + h),
            ],
            color,
            Stroke::NONE,
        )
    };
    let bar = |x0: f32, x1: f32| {
        Shape::rect_filled(
            Rect::from_min_max(pos2(x0, c.y - h), pos2(x1, c.y + h)),
            0.0,
            color,
        )
    };
    let w = h * 0.36;
    let painter = ui.painter();
    match glyph {
        Glyph::Play => painter.add(triangle(c.x + h * 0.1, 1.0)),
        Glyph::Pause => {
            painter.add(bar(c.x - h * 0.62, c.x - h * 0.62 + w * 1.4));
            painter.add(bar(c.x + h * 0.62 - w * 1.4, c.x + h * 0.62))
        }
        Glyph::Previous => {
            painter.add(bar(c.x - h * 0.85, c.x - h * 0.85 + w));
            painter.add(triangle(c.x + h * 0.15, -1.0))
        }
        Glyph::Next => {
            painter.add(bar(c.x + h * 0.85 - w, c.x + h * 0.85));
            painter.add(triangle(c.x - h * 0.15, 1.0))
        }
    };
}

/// Draws the player into `rect` and returns a command when a button or the bar was
/// clicked. `art` is the decoded album art, if any.
pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    cfg: &Config,
    track: Option<&Track>,
    position_ms: i64,
    art: Option<&TextureHandle>,
    scale: f32,
) -> Option<Command> {
    let track = track?;
    let p = &cfg.player;
    let o = p.opacity;
    let at = |spot: inlay_core::config::Spot| rect.min + vec2(spot.x, spot.y) * scale;

    if p.show_art && p.art_size > 0.0 {
        let side = p.art_size * scale;
        let frame = Rect::from_min_size(at(p.art), vec2(side, side));
        let radius = CornerRadius::from(side * p.art_radius);
        let shape = match art {
            Some(texture) => RectShape::filled(frame, radius, Color32::WHITE.gamma_multiply(o))
                .with_texture(
                    texture.id(),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                ),
            None => RectShape::filled(frame, radius, Color32::WHITE.gamma_multiply(0.09 * o)),
        };
        ui.painter().add(shape);
    }

    let title = if p.short_title {
        short_title(&track.title)
    } else {
        &track.title
    };
    text(ui, rect.min, &p.title, title, cfg, scale);
    text(ui, rect.min, &p.artist, &track.artist, cfg, scale);

    let mut command = None;

    if p.show_controls {
        let size = p.button_size * scale;
        let color = color_of(&p.button_color, &cfg.theme).gamma_multiply(o);
        let play = if track.playing {
            Glyph::Pause
        } else {
            Glyph::Play
        };
        let buttons = [
            ("previous", p.previous, Glyph::Previous, Command::Previous),
            ("play", p.play, play, Command::PlayPause),
            ("next", p.next, Glyph::Next, Command::Next),
        ];
        for (name, spot, glyph, cmd) in buttons {
            let button = Rect::from_min_size(at(spot), vec2(size, size));
            let id = ui.id().with(name);
            let response = ui.interact(button, id, Sense::CLICK);
            if response.hovered() {
                ui.painter().circle_filled(
                    button.center(),
                    size / 2.0,
                    Color32::WHITE.gamma_multiply(0.14 * o),
                );
            }
            let share = if glyph == Glyph::Play || glyph == Glyph::Pause {
                PLAY_GLYPH_SHARE
            } else {
                GLYPH_SHARE
            };
            draw_glyph(ui, glyph, button, size * share, color);
            if response.clicked() {
                command = Some(cmd);
            }
        }
    }

    if p.show_progress {
        let thick = p.bar_height * scale;
        let width = p.bar_width * scale;
        let bar = Rect::from_min_size(at(p.bar), vec2(width, thick));
        let radius = CornerRadius::from(thick / 2.0);
        ui.painter().rect_filled(
            bar,
            radius,
            Color32::WHITE.gamma_multiply(p.bar_track_opacity * o),
        );
        if track.duration_ms > 0 {
            let fraction = (position_ms as f64 / track.duration_ms as f64).clamp(0.0, 1.0) as f32;
            if fraction > 0.0 {
                // At least a dot wide, so the round ends do not squash.
                let done = (width * fraction).max(thick.min(width));
                ui.painter().rect_filled(
                    Rect::from_min_size(bar.min, vec2(done, thick)),
                    radius,
                    color_of(&p.bar_color, &cfg.theme).gamma_multiply(o),
                );
            }
        }
        // A thin bar is hard to hit, so the click area is taller than it.
        let hit = bar.expand2(vec2(0.0, BAR_SLOP * scale));
        let response = ui.interact(hit, ui.id().with("player-bar"), Sense::CLICK);
        if response.clicked()
            && track.duration_ms > 0
            && let Some(pos) = response.interact_pointer_pos()
        {
            command = Some(Command::Seek(seek_ms(
                pos.x,
                bar.left(),
                width,
                track.duration_ms,
            )));
        }

        text(
            ui,
            rect.min,
            &p.elapsed,
            &clock_text(position_ms),
            cfg,
            scale,
        );
        text(
            ui,
            rect.min,
            &p.total,
            &clock_text(track.duration_ms),
            cfg,
            scale,
        );
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_minutes_and_two_digit_seconds() {
        assert_eq!(clock_text(0), "0:00");
        assert_eq!(clock_text(65_900), "1:05");
        assert_eq!(clock_text(-5), "0:00");
    }

    #[test]
    fn a_click_maps_onto_the_track() {
        assert_eq!(seek_ms(150.0, 100.0, 100.0, 200_000), 100_000);
        assert_eq!(seek_ms(50.0, 100.0, 100.0, 200_000), 0);
        assert_eq!(seek_ms(500.0, 100.0, 100.0, 200_000), 200_000);
        assert_eq!(seek_ms(1.0, 0.0, 0.0, 200_000), 0);
    }
}
