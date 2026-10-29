//! CPU, memory, graphics, drives, network and battery, each with an optional meter.

use inlay_core::color::{Rgba, with_alpha};
use inlay_core::config::{
    Align, Direction, Label, Letters, StatsMeter, StatsNames, StatsOrientation, StatsStructure,
};
use inlay_core::stats::{
    Block, Composed, Grid, Item, MeterSpec, Peaks, Size, compose, items, layout, widest_value,
};
use windows::core::Result;

use super::{
    Ctx, Part, Tick, Wake, Widget, draw_card, draw_placed, label_color, named_color, place_label,
};
use crate::gfx::{Gfx, rect};

/// A reading is never cut short: the window grows to hold it.
const ROOM: f32 = 4000.0;

#[derive(Default)]
pub struct Stats {
    /// The readings as currently drawn.
    items: Vec<Item>,
    peaks: Peaks,
    /// Every reading drawn last time, for dragging in edit mode.
    parts: Vec<Part>,
}

/// A label laid out by the widget: only its look is the user's to choose.
fn upright(label: &Label, x: f32, y: f32) -> Label {
    Label {
        x,
        y,
        align: Align::Left,
        direction: Direction::Horizontal,
        letters: Letters::Upright,
        ..label.clone()
    }
}

fn size_of(g: &mut Gfx, text: &str, label: &Label, ctx: &Ctx) -> Result<Size> {
    let (w, h) = place_label(g, text, label, ctx, ROOM)?.size();
    Ok(Size { w, h })
}

impl Widget for Stats {
    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let items = items(&ctx.cfg.stats, ctx.stats, &mut self.peaks);
        let dirty = items != self.items;
        self.items = items;
        // The sampler's message wakes the widget; nothing here moves on its own.
        Tick {
            dirty,
            wake: Wake::Idle,
        }
    }

    fn reset(&mut self) {
        self.items.clear();
        self.parts.clear();
    }

    fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let s = &ctx.cfg.stats;
        if s.card {
            draw_card(g, w, h, ctx, s.opacity);
        }
        let shadow = ctx.cfg.theme.text_shadow && !s.card;

        let name_label = upright(&s.name, 0.0, 0.0);
        let value_label = upright(&s.value, 0.0, 0.0);
        let meter = MeterSpec {
            kind: s.meter,
            length: s.meter_length,
            thickness: s.meter_thickness,
            ring: s.ring_size,
        };
        // The rows of a drives reading stack the way the readings run.
        let rows_grid = Grid {
            structure: StatsStructure::Strip,
            orientation: if s.structure == StatsStructure::Strip
                && s.orientation == StatsOrientation::Across
            {
                StatsOrientation::Across
            } else {
                StatsOrientation::Down
            },
            columns: 1,
            gap: s.gap / 2.0,
            padding: 0.0,
        };

        // First every row is measured, then everything is placed.
        let mut composed: Vec<Vec<Composed>> = Vec::with_capacity(self.items.len());
        let mut stacks = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let rd = s.reading(item.id);
            let widest = widest_value(item.id, rd.value, s.net_units);
            let widest = size_of(g, widest, &value_label, ctx)?;
            let mut rows = Vec::with_capacity(item.rows.len());
            for row in &item.rows {
                let name = if s.names == StatsNames::Hidden {
                    Size { w: 0.0, h: 0.0 }
                } else {
                    size_of(g, &row.name, &name_label, ctx)?
                };
                let value = size_of(g, &row.value, &value_label, ctx)?;
                // Wide enough for any value, so a changing one moves nothing.
                let value = Size {
                    w: value.w.max(widest.w),
                    h: value.h.max(widest.h),
                };
                rows.push(compose(Block { name, value }, s.names, meter));
            }
            let sizes: Vec<Size> = rows.iter().map(|c| c.size).collect();
            stacks.push(layout(&rows_grid, &sizes, &[]));
            composed.push(rows);
        }
        let sizes: Vec<Size> = stacks.iter().map(|l| l.size).collect();
        let free: Vec<(f32, f32)> = self
            .items
            .iter()
            .map(|item| {
                let rd = s.reading(item.id);
                (rd.x, rd.y)
            })
            .collect();
        let laid = layout(
            &Grid {
                structure: s.structure,
                orientation: s.orientation,
                columns: s.columns,
                gap: s.gap,
                padding: s.padding,
            },
            &sizes,
            &free,
        );

        let alpha = |c: Rgba, a: f32| with_alpha(c, a * s.opacity);
        let warn = named_color(&s.warn_color, ctx);
        self.parts.clear();
        for (i, item) in self.items.iter().enumerate() {
            let (ix, iy) = laid.at[i];
            let cell = laid.cell[i];
            let color = if item.color.is_empty() {
                &s.meter_color
            } else {
                &item.color
            };
            for (j, row) in item.rows.iter().enumerate() {
                let (rx, ry) = stacks[i].at[j];
                let (x, y) = (ix + rx, iy + ry);
                let c = &composed[i][j];
                if s.names != StatsNames::Hidden {
                    let label = upright(&s.name, x + c.name.0, y + c.name.1);
                    let placed = place_label(g, &row.name, &label, ctx, ROOM)?;
                    draw_placed(g, &placed, label_color(&label, ctx, s.opacity), shadow);
                }
                let label = upright(&s.value, x + c.value.0, y + c.value.1);
                let placed = place_label(g, &row.value, &label, ctx, ROOM)?;
                let text = if row.warn {
                    alpha(warn, label.opacity)
                } else if item.color.is_empty() {
                    label_color(&label, ctx, s.opacity)
                } else {
                    alpha(named_color(&item.color, ctx), label.opacity)
                };
                draw_placed(g, &placed, text, shadow);

                let Some([mx, my, mw, mh]) = c.meter else {
                    continue;
                };
                let (mx, my) = (x + mx, y + my);
                let track = [1.0, 1.0, 1.0, s.track_opacity * s.opacity];
                let fill = if row.warn {
                    alpha(warn, 1.0)
                } else {
                    alpha(named_color(color, ctx), 1.0)
                };
                match s.meter {
                    StatsMeter::None => {}
                    StatsMeter::Bar => {
                        g.fill_round(rect(mx, my, mw, mh), mh / 2.0, track);
                        let filled = mw * row.fraction;
                        if filled > 0.0 {
                            g.fill_round(rect(mx, my, filled.max(mh), mh), mh / 2.0, fill);
                        }
                    }
                    StatsMeter::Ring => {
                        let width = s.meter_thickness;
                        let centre = (mx + mw / 2.0, my + mh / 2.0);
                        let radius = (mw - width) / 2.0;
                        g.stroke_circle(centre.0, centre.1, radius, width, track);
                        g.stroke_arc(centre, radius, 0.0, row.fraction, width, fill)?;
                    }
                }
            }
            self.parts
                .push((item.id as u8, rect(ix, iy, cell.w, cell.h)));
        }
        Ok(())
    }
}
