//! The lyrics widget's drawing: the line being sung in the middle, its neighbours fading
//! away above and below.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::text::{Galley, LayoutJob, TextFormat};
use eframe::egui::{self, Align2, Color32, FontId, Painter, Rect, vec2};
use inlay_core::clock::find_line_index;
use inlay_core::config::{Align, Config, LyricsMode};
use inlay_core::timing::Line;

use crate::clock::color_of;
use crate::fonts;
use crate::model::Lyrics;

/// How far the shadow sits from the text, before scaling.
const SHADOW_OFFSET: f32 = 1.5;
/// A run of coloured text: what it says and what colour it is drawn in.
type Segment = (String, Color32);

/// The lines to lay out around `centre` (the line being sung, if one has started) out of
/// `n`: `before` above it and `after` below. With no centre the first `after` lines show.
fn visible(n: usize, centre: Option<usize>, before: usize, after: usize) -> Range<usize> {
    match centre {
        Some(c) => c.saturating_sub(before)..(c + after + 1).min(n),
        None => 0..(after + 1).min(n),
    }
}

/// How much of its strength a line `distance` lines from the centre keeps.
fn fade(distance: usize, falloff: f32) -> f32 {
    (1.0 - falloff.clamp(0.0, 0.9)).powi(distance.saturating_sub(1) as i32)
}

/// The top of each of `heights` stacked with `gap` between them: the `anchor` item centred
/// on `cy` and the rest outward from it, or the whole column centred when there is none.
fn tops(heights: &[f32], anchor: Option<usize>, gap: f32, cy: f32) -> Vec<f32> {
    let mut tops = vec![0.0; heights.len()];
    match anchor.filter(|&k| k < heights.len()) {
        Some(k) => {
            tops[k] = cy - heights[k] / 2.0;
            let mut y = tops[k];
            for (t, h) in tops[..k].iter_mut().zip(&heights[..k]).rev() {
                y -= gap + h;
                *t = y;
            }
            let mut y = tops[k] + heights[k] + gap;
            for (t, h) in tops[k + 1..].iter_mut().zip(&heights[k + 1..]) {
                *t = y;
                y += h + gap;
            }
        }
        None => {
            let total: f32 =
                heights.iter().sum::<f32>() + gap * heights.len().saturating_sub(1) as f32;
            let mut y = cy - total / 2.0;
            for (t, h) in tops.iter_mut().zip(heights) {
                *t = y;
                y += h + gap;
            }
        }
    }
    tops
}

/// Milliseconds until the picture can next change, from the track time `t`: the next line
/// starting, the lit line ending and, while words are followed, the next word starting.
fn next_change(
    lines: &[Line],
    t: i64,
    centre: Option<usize>,
    lit: bool,
    words: bool,
) -> Option<i64> {
    let mut next = lines.get(centre.map_or(0, |i| i + 1)).map(|l| l.start_ms);
    if let Some(line) = centre.map(|i| &lines[i]).filter(|_| lit) {
        let ends = std::iter::once(line.end_ms);
        let starts = line.words.iter().map(|w| w.start_ms).filter(|_| words);
        next = ends.chain(starts).filter(|&m| m > t).chain(next).min();
    }
    next.map(|n| (n - t).max(1))
}

/// The coloured runs of the line being sung. A line followed word by word, or filling in
/// progress mode, shows the words already started in `sung` and the rest in `unsung`.
fn segments(
    line: &Line,
    t: i64,
    by_word: bool,
    progress: bool,
    active: Color32,
    word: Color32,
    unsung: Color32,
) -> Vec<Segment> {
    if !by_word && !progress {
        return vec![(line.text.clone(), active)];
    }
    let sung = if by_word { word } else { active };
    let last = line.words.len().saturating_sub(1);
    line.words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let space = if w.joined || i == last { "" } else { " " };
            (
                format!("{}{space}", w.text),
                if w.start_ms <= t { sung } else { unsung },
            )
        })
        .collect()
}

fn layout(
    painter: &Painter,
    segs: &[Segment],
    font: &FontId,
    wrap: f32,
    align: Align,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = match align {
        Align::Left => egui::Align::LEFT,
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
    };
    for (text, color) in segs {
        job.append(text, 0.0, TextFormat::simple(font.clone(), *color));
    }
    painter.layout_job(job)
}

/// One line to draw: its coloured runs, the size to draw them at and how strong it is.
struct Item {
    segs: Vec<Segment>,
    size: f32,
    strength: f32,
}

/// Draws the lyrics around the line sung at `position_ms` into `rect`. Returns how long
/// until something on it changes (the next line, or the next word starting), or None
/// when nothing will change until the track moves on.
pub fn draw(
    painter: &Painter,
    rect: Rect,
    cfg: &Config,
    lyrics: &Lyrics,
    position_ms: i64,
    scale: f32,
) -> Option<Duration> {
    let l = &cfg.lyrics;
    let theme = &cfg.theme;
    let o = l.opacity;
    let pad = (if l.card { 22.0 } else { 6.0 }) * scale;
    let (before, after) = (l.lines_before as usize, l.lines_after as usize);
    let inactive = color_of(&l.inactive_color, theme);
    let painter = painter.with_clip_rect(rect);

    let t = position_ms + i64::from(l.offset_ms);
    let (items, anchor, wake) = match lyrics {
        Lyrics::None | Lyrics::Loading | Lyrics::Instrumental => return None,
        Lyrics::Missing | Lyrics::Unavailable => {
            let text = if matches!(lyrics, Lyrics::Missing) {
                "No lyrics for this song"
            } else {
                "Lyrics unavailable offline"
            };
            let (anchor, x) = match l.align {
                Align::Left => (Align2::LEFT_CENTER, rect.left() + pad),
                Align::Center => (Align2::CENTER_CENTER, rect.center().x),
                Align::Right => (Align2::RIGHT_CENTER, rect.right() - pad),
            };
            let font = FontId::new(l.size * scale, fonts::family(&l.font, &theme.font));
            painter.text(
                egui::pos2(x, rect.center().y),
                anchor,
                text,
                font,
                inactive.gamma_multiply(l.inactive_opacity * o),
            );
            return None;
        }
        Lyrics::Plain(lines) => {
            let range = visible(lines.len(), None, 0, before + after);
            let color = color_of(&l.active_color, theme).gamma_multiply(o);
            let items = lines[range]
                .iter()
                .map(|s| Item {
                    segs: vec![(s.clone(), color)],
                    size: l.size * scale,
                    strength: 1.0,
                })
                .collect::<Vec<_>>();
            // Not timed, so nothing moves.
            (items, None, None)
        }
        Lyrics::Synced(lines) => {
            if lines.is_empty() {
                return None;
            }
            let centre = find_line_index(lines, t);
            let current = centre.map(|i| &lines[i]);
            let lit = current.is_some_and(|c| t < c.end_ms);
            // Only words whose times came from a source are followed.
            let by_word = lit && l.word_sync && current.is_some_and(Line::word_timed);
            let progress = l.mode == LyricsMode::Progress;
            let active = color_of(&l.active_color, theme).gamma_multiply(o);
            let word = color_of(&l.word_color, theme).gamma_multiply(o);
            let unsung = inactive.gamma_multiply(l.unsung_opacity * o);

            let range = visible(lines.len(), centre, before, after);
            let items = range
                .clone()
                .map(|i| {
                    let distance = centre.map_or(i + 1, |c| i.abs_diff(c));
                    let strength = fade(distance, l.falloff);
                    if centre == Some(i) {
                        let segs = if lit {
                            segments(&lines[i], t, by_word, progress, active, word, unsung)
                        } else {
                            vec![(
                                lines[i].text.clone(),
                                inactive.gamma_multiply(l.inactive_opacity * o),
                            )]
                        };
                        Item {
                            segs,
                            size: l.size * scale * l.active_scale,
                            strength: 1.0,
                        }
                    } else {
                        let color = inactive.gamma_multiply(l.inactive_opacity * o * strength);
                        Item {
                            segs: vec![(lines[i].text.clone(), color)],
                            size: l.size * scale,
                            strength,
                        }
                    }
                })
                .collect::<Vec<_>>();
            let anchor = centre.map(|c| c - range.start);
            let wake = next_change(lines, t, centre, lit, by_word || (progress && lit));
            (items, anchor, wake)
        }
    };
    if items.is_empty() {
        return None;
    }
    if l.card {
        painter.rect_filled(
            rect,
            theme.card_radius.min(rect.width().min(rect.height()) / 2.0),
            color_of(&theme.card_color, theme).gamma_multiply(theme.card_opacity * o),
        );
    }

    let wrap = (rect.width() - 2.0 * pad).max(1.0);
    let (x, halign) = match l.align {
        Align::Left => (rect.left() + pad, Align::Left),
        Align::Center => (rect.center().x, Align::Center),
        Align::Right => (rect.right() - pad, Align::Right),
    };
    let family = fonts::family(&l.font, &theme.font);
    let galleys: Vec<_> = items
        .iter()
        .map(|it| {
            layout(
                &painter,
                &it.segs,
                &FontId::new(it.size, family.clone()),
                wrap,
                halign,
            )
        })
        .collect();
    let heights: Vec<f32> = galleys.iter().map(|g| g.size().y).collect();
    let tops = tops(
        &heights,
        anchor,
        l.size * scale * l.line_gap,
        rect.center().y,
    );

    let shadow = color_of(&l.shadow_color, theme);
    for ((item, galley), (top, height)) in
        items.iter().zip(galleys).zip(tops.into_iter().zip(heights))
    {
        if top + height < rect.top() || top > rect.bottom() {
            continue;
        }
        let at = egui::pos2(x, top);
        if l.shadow_opacity > 0.0 {
            // ponytail: the stroke (stroke_width, stroke_color) is not drawn; egui has no
            // text outline, so it would need the glyphs stamped around the text.
            let color = shadow.gamma_multiply(l.shadow_opacity * o * item.strength);
            let segs: Vec<Segment> = item.segs.iter().map(|(s, _)| (s.clone(), color)).collect();
            let font = FontId::new(item.size, family.clone());
            let dark = layout(&painter, &segs, &font, wrap, halign);
            painter.galley(at + vec2(SHADOW_OFFSET, SHADOW_OFFSET) * scale, dark, color);
        }
        painter.galley(at, galley, Color32::WHITE);
    }
    // ponytail: a word that is part way through is coloured whole, not filled left to
    // right, and the scroll between lines (scroll_ms) is a jump; a gradient fill and an
    // eased scroll need per-frame repaints and a retained scroll position.
    // ponytail: weight is not applied; egui picks the weight from the font file.
    wake.map(|ms| Duration::from_millis(ms.unsigned_abs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use inlay_core::timing::Word;

    fn line(start_ms: i64, end_ms: i64, words: &[(i64, i64)]) -> Line {
        let words: Vec<Word> = words
            .iter()
            .map(|&(s, e)| Word {
                text: "w".into(),
                start_ms: s,
                end_ms: e,
                synthesized: false,
                joined: false,
            })
            .collect();
        Line {
            start_ms,
            end_ms,
            text: vec!["w"; words.len()].join(" "),
            words,
        }
    }

    #[test]
    fn lines_around_the_centre_are_clamped_to_the_song() {
        assert_eq!(visible(10, Some(5), 2, 2), 3..8);
        assert_eq!(visible(10, Some(0), 2, 2), 0..3);
        assert_eq!(visible(10, Some(9), 2, 2), 7..10);
        // Before the first line the opening lines wait below.
        assert_eq!(visible(10, None, 2, 2), 0..3);
        assert_eq!(visible(2, None, 2, 2), 0..2);
        assert_eq!(visible(0, None, 2, 2), 0..0);
    }

    #[test]
    fn each_line_further_out_is_fainter() {
        assert_eq!(fade(0, 0.25), 1.0);
        assert_eq!(fade(1, 0.25), 1.0);
        assert!((fade(2, 0.25) - 0.75).abs() < 1e-6);
        assert!((fade(3, 0.25) - 0.5625).abs() < 1e-6);
        // Falloff is capped at 0.9.
        assert!((fade(2, 5.0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn lines_stack_outward_from_the_centred_one() {
        let t = tops(&[10.0, 20.0, 10.0], Some(1), 4.0, 100.0);
        assert_eq!(t, [76.0, 90.0, 114.0]);
        // No centre: the column as a whole is centred.
        let t = tops(&[10.0, 10.0], None, 4.0, 100.0);
        assert_eq!(t, [88.0, 102.0]);
        assert!(tops(&[], None, 4.0, 100.0).is_empty());
    }

    #[test]
    fn the_wait_ends_at_the_next_line_or_when_the_lit_line_ends() {
        let lines = [
            line(1000, 2000, &[(1000, 1500), (1500, 2000)]),
            line(5000, 6000, &[(5000, 6000)]),
        ];
        // Before the first line: wait for it.
        assert_eq!(next_change(&lines, 400, None, false, false), Some(600));
        // Singing line 0 by lines: until it ends.
        assert_eq!(next_change(&lines, 1200, Some(0), true, false), Some(800));
        // By words: until the next word starts.
        assert_eq!(next_change(&lines, 1200, Some(0), true, true), Some(300));
        // In the gap after it: until line 1.
        assert_eq!(next_change(&lines, 3000, Some(0), false, false), Some(2000));
        // After the last line there is nothing to wait for.
        assert_eq!(next_change(&lines, 7000, Some(1), false, false), None);
    }

    #[test]
    fn started_words_are_sung_and_the_rest_unsung() {
        let l = line(0, 3000, &[(0, 1000), (1000, 2000), (2000, 3000)]);
        let (a, w, u) = (Color32::RED, Color32::GREEN, Color32::GRAY);
        let colors = |by_word, progress| -> Vec<Color32> {
            segments(&l, 1500, by_word, progress, a, w, u)
                .iter()
                .map(|s| s.1)
                .collect()
        };
        assert_eq!(colors(true, false), [w, w, u]);
        assert_eq!(colors(false, true), [a, a, u]);
        assert_eq!(
            segments(&l, 1500, false, false, a, w, u),
            [("w w w".to_owned(), a)]
        );
    }
}
