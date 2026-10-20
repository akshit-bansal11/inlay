//! The spectrum visualizer's drawing: bars, mirrored bars or a wave, in the configured
//! colours, turned by `rotation`.

use eframe::egui::{Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, vec2};
use inlay_core::config::{Config, VisualizerColor, VisualizerStyle};

use crate::clock::color_of;

/// Levels below this are silence.
const AT_REST: f32 = 0.003;
/// A bar with no sound in it is drawn this tall, as a dot, when `show_idle` is on.
const MIN_BAR_HEIGHT: f32 = 3.0;
/// Room kept around the bars when the widget is a card.
const CARD_PAD: f32 = 18.0;

/// How many of `wanted` bars fit side by side in `room`, at least one.
fn fitting(wanted: usize, room: f32, bar_width: f32, gap: f32) -> usize {
    // A gap sits between two bars, so the last bar needs none after it.
    let fit = ((room + gap) / (bar_width + gap)) as usize;
    wanted.min(fit).max(1)
}

/// The level shown by bar `i` of `n` bars, from `levels` spread over them.
/// Each bar takes the loudest level of the slice of `levels` it covers, so
/// leaving bars out loses no treble.
fn sample(levels: &[f32], i: usize, n: usize) -> f32 {
    if levels.is_empty() || n == 0 {
        return 0.0;
    }
    let lo = i * levels.len() / n;
    let hi = ((i + 1) * levels.len() / n).clamp(lo + 1, levels.len());
    levels[lo..hi].iter().fold(0.0, |m, &l| m.max(l))
}

/// The level of bar `i` of `bars`, with bass in the middle when `symmetric`.
fn bar_level(levels: &[f32], i: usize, bars: usize, symmetric: bool) -> f32 {
    if !symmetric {
        return sample(levels, i, bars);
    }
    let half = bars / 2;
    let band = if i < half { half - 1 - i } else { i - half };
    sample(levels, band, bars.div_ceil(2))
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        ch(a.r(), b.r()),
        ch(a.g(), b.g()),
        ch(a.b(), b.b()),
        ch(a.a(), b.a()),
    )
}

/// Where a point of the picture, drawn upright in a box `rotation` turns into
/// `real`, ends up. 90 is a quarter turn clockwise.
fn turn(real: Rect, rotation: u32, x: f32, y: f32) -> Pos2 {
    let (w, h) = (real.width(), real.height());
    let (dx, dy) = match rotation {
        90 => (w - y, x),
        180 => (w - x, h - y),
        270 => (y, h - x),
        _ => (x, y),
    };
    real.min + vec2(dx, dy)
}

/// Draws the spectrum `levels` (0..1 per bar) into `rect` in the configured style.
pub fn draw(painter: &Painter, rect: Rect, cfg: &Config, levels: &[f32], scale: f32) {
    let v = &cfg.visualizer;
    if v.bars == 0 || (!v.show_idle && levels.iter().all(|&l| l < AT_REST)) {
        return;
    }
    if v.card {
        let theme = &cfg.theme;
        painter.rect_filled(
            rect,
            theme.card_radius.min(rect.width().min(rect.height()) / 2.0),
            color_of(&theme.card_color, theme).gamma_multiply(theme.card_opacity * v.opacity),
        );
    }
    // Turned, the same picture is drawn in a box as wide as this one is tall.
    let rotation = v.rotation;
    let (w, h) = if matches!(rotation, 90 | 270) {
        (rect.height(), rect.width())
    } else {
        (rect.width(), rect.height())
    };
    let pad = if v.card { CARD_PAD * scale } else { 0.0 };
    let (top, width, height) = (pad, w - 2.0 * pad, h - 2.0 * pad);
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let accent = color_of("accent", &cfg.theme);
    let (a, b) = match v.color {
        VisualizerColor::Accent => (accent, mix(accent, Color32::WHITE, 0.45)),
        VisualizerColor::Solid => (
            color_of(&v.color_a, &cfg.theme),
            color_of(&v.color_a, &cfg.theme),
        ),
        VisualizerColor::Gradient => (
            color_of(&v.color_a, &cfg.theme),
            color_of(&v.color_b, &cfg.theme),
        ),
    };
    let color_at =
        |x: f32| mix(a, b, ((x - pad) / width).clamp(0.0, 1.0)).gamma_multiply(v.opacity);

    // Bars keep their own width and gap whatever their number; those that do not fit
    // are left out and the rest sit in the middle.
    let bar_width = (v.bar_width * scale).min(width);
    let gap = v.bar_gap * scale;
    let pitch = bar_width + gap;
    let bars = fitting(v.bars as usize, width, bar_width, gap);
    let run = bars as f32 * pitch - gap;
    let start = pad + ((width - run) / 2.0).max(0.0);
    let floor = if v.show_idle {
        MIN_BAR_HEIGHT.min(height)
    } else {
        0.0
    };
    // Flipping sideways reads the bands in reverse.
    let bar_height = |i: usize| {
        let slot = if v.flip_x { bars - 1 - i } else { i };
        (bar_level(levels, slot, bars, v.symmetric).clamp(0.0, 1.0) * height).max(floor)
    };

    if v.style == VisualizerStyle::Wave {
        // Flipping upside down hangs the shape from the top edge.
        let (base, rise) = if v.flip_y {
            (top, 1.0)
        } else {
            (top + height, -1.0)
        };
        // The curve, starting and ending on the base line.
        let mut curve = vec![(start, base)];
        curve.extend((0..bars).map(|i| {
            (
                start + pitch * i as f32 + bar_width / 2.0,
                base + rise * bar_height(i),
            )
        }));
        curve.push((start + run, base));
        // ponytail: straight segments between bars, not a smoothed curve; fine at 100+ bars.
        let mut mesh = Mesh::default();
        for &(x, y) in &curve {
            let c = color_at(x);
            mesh.colored_vertex(turn(rect, rotation, x, y), c);
            mesh.colored_vertex(turn(rect, rotation, x, base), c);
        }
        for j in 0..curve.len() as u32 - 1 {
            let (t0, b0, t1, b1) = (2 * j, 2 * j + 1, 2 * j + 2, 2 * j + 3);
            mesh.add_triangle(t0, b0, t1);
            mesh.add_triangle(b0, b1, t1);
        }
        painter.add(Shape::mesh(mesh));
        return;
    }

    for i in 0..bars {
        let bh = bar_height(i);
        if bh < 0.5 {
            continue;
        }
        let x = start + pitch * i as f32;
        let y = match v.style {
            VisualizerStyle::Mirror => top + (height - bh) / 2.0,
            _ if v.flip_y => top,
            _ => top + height - bh,
        };
        let corners = Rect::from_two_pos(
            turn(rect, rotation, x, y),
            turn(rect, rotation, x + bar_width, y + bh),
        );
        painter.rect_filled(
            corners,
            CornerRadius::from((bar_width * v.radius).min(bh / 2.0)),
            color_at(x + bar_width / 2.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn bars_that_do_not_fit_are_left_out_and_never_squeezed() {
        // Ten wide with a gap of six: 16 each, and the last needs no gap.
        assert_eq!(fitting(120, 160.0, 10.0, 6.0), 10);
        assert_eq!(fitting(120, 170.0, 10.0, 6.0), 11);
        assert_eq!(fitting(4, 2000.0, 10.0, 6.0), 4);
        assert_eq!(fitting(120, 3.0, 10.0, 6.0), 1);
    }

    #[test]
    fn a_symmetric_layout_puts_bass_in_the_middle() {
        let levels = [1.0, 0.5, 0.25, 0.1];
        let shown: Vec<f32> = (0..8).map(|i| bar_level(&levels, i, 8, true)).collect();
        assert_eq!(shown, [0.1, 0.25, 0.5, 1.0, 1.0, 0.5, 0.25, 0.1]);
    }

    #[test]
    fn fewer_bars_than_levels_keep_the_loudest_of_each_slice() {
        let levels = [0.1, 0.9, 0.2, 0.3];
        assert_eq!(sample(&levels, 0, 2), 0.9);
        assert_eq!(sample(&levels, 1, 2), 0.3);
        assert_eq!(sample(&levels, 0, 4), 0.1);
        assert_eq!(sample(&[], 0, 4), 0.0);
        // More bars than levels repeat a level.
        assert_eq!(sample(&[0.5], 3, 4), 0.5);
    }

    #[test]
    fn a_quarter_turn_maps_the_corners_of_the_upright_box() {
        let real = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 40.0));
        assert_eq!(turn(real, 0, 3.0, 4.0), pos2(3.0, 4.0));
        assert_eq!(turn(real, 90, 3.0, 4.0), pos2(96.0, 3.0));
        assert_eq!(turn(real, 180, 3.0, 4.0), pos2(97.0, 36.0));
        assert_eq!(turn(real, 270, 3.0, 4.0), pos2(4.0, 37.0));
    }
}
