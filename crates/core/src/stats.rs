//! The System widget's logic: what is read, how it is worked out and
//! written, and where each reading goes. No Windows types: the sampler in the
//! app crate fills a [`Sample`] and this turns it into text and positions.

use std::collections::HashMap;

use crate::config::{
    NetUnits, StatReading, StatsCfg, StatsMeter, StatsNames, StatsOrientation, StatsStructure,
    ValueFormat,
};

/// Something used out of a total, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

impl Usage {
    /// 0 to 100; 0 when the total is unknown.
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f64 / self.total as f64 * 100.0).clamp(0.0, 100.0) as f32
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub letter: char,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Battery {
    pub percent: f32,
    pub charging: bool,
}

/// One round of readings. `None` is a reading that could not be taken.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub cpu: Option<f32>,
    pub ram: Option<Usage>,
    pub gpu: Option<f32>,
    pub vram: Option<Usage>,
    pub disks: Vec<Disk>,
    /// Bytes per second.
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub battery: Option<Battery>,
}

/// Processor times as `GetSystemTimes` reports them: `kernel` includes `idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub kernel: u64,
    pub user: u64,
}

/// How busy the processors were between two readings. `None` when no time
/// passed or a counter went backwards.
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> Option<f32> {
    if now.idle < prev.idle || now.kernel < prev.kernel || now.user < prev.user {
        return None;
    }
    let total = (now.kernel - prev.kernel) + (now.user - prev.user);
    if total == 0 {
        return None;
    }
    let busy = total.saturating_sub(now.idle - prev.idle);
    Some((busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32)
}

/// Bytes per second between two counter readings; 0 when the counter reset.
pub fn byte_rate(prev: u64, now: u64, secs: f64) -> f64 {
    if now < prev || secs <= 0.0 {
        return 0.0;
    }
    (now - prev) as f64 / secs
}

/// Bytes received and sent since `prev` across adapters, both lists being
/// (interface LUID, in, out). An adapter absent from `prev` is new and adds 0,
/// and a counter that went down adds 0, so a late adapter's lifetime octets
/// never show as one spike.
pub fn octet_deltas(prev: &[(u64, u64, u64)], now: &[(u64, u64, u64)]) -> (u64, u64) {
    let (mut down, mut up) = (0u64, 0u64);
    for &(key, d, u) in now {
        if let Some(&(_, pd, pu)) = prev.iter().find(|p| p.0 == key) {
            down += d.saturating_sub(pd);
            up += u.saturating_sub(pu);
        }
    }
    (down, up)
}

/// Steps to the next unit once the value, before rounding, reaches 1000, so
/// 999.96 shows as "1000.0" rather than "1.0" of the next unit. Bits are the
/// bytes times eight.
pub fn format_rate(bytes_per_sec: f64, units: NetUnits) -> String {
    let bytes = if bytes_per_sec.is_finite() && bytes_per_sec > 0.0 {
        bytes_per_sec
    } else {
        0.0
    };
    let (mut value, names) = match units {
        NetUnits::Bytes => (bytes / 1000.0, ["KB/s", "MB/s", "GB/s"]),
        NetUnits::Bits => (bytes * 8.0 / 1000.0, ["Kbps", "Mbps", "Gbps"]),
    };
    let mut step = 0;
    while value >= 1000.0 && step < names.len() - 1 {
        value /= 1000.0;
        step += 1;
    }
    format!("{value:.1} {}", names[step])
}

fn number(v: f64) -> String {
    if v >= 100.0 {
        return format!("{v:.0}");
    }
    let s = format!("{v:.1}");
    if s.ends_with(".0") {
        s[..s.len() - 2].to_owned()
    } else {
        s
    }
}

/// "9.3 / 16 GB": the unit follows the total, in steps of 1024.
pub fn format_amount(used: u64, total: u64) -> String {
    if total == 0 {
        return "–".to_owned();
    }
    const MIB: f64 = 1_048_576.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;
    let (divisor, unit) = if total as f64 >= TIB {
        (TIB, "TB")
    } else if total as f64 >= GIB {
        (GIB, "GB")
    } else {
        (MIB, "MB")
    };
    format!(
        "{} / {} {unit}",
        number(used as f64 / divisor),
        number(total as f64 / divisor)
    )
}

pub fn format_percent(p: f32) -> String {
    format!("{}%", p.clamp(0.0, 100.0).round() as u32)
}

/// The widest text a reading can show, to size its cell so a changing value
/// never moves its neighbours.
pub fn widest_value(r: StatReading, format: ValueFormat, units: NetUnits) -> &'static str {
    match r {
        StatReading::Cpu | StatReading::Gpu | StatReading::Battery => "100%",
        StatReading::Ram | StatReading::Vram | StatReading::Disks => match format {
            ValueFormat::Percent => "100%",
            ValueFormat::Amount => "99.9 / 99.9 GB",
        },
        StatReading::Down | StatReading::Up => match units {
            NetUnits::Bytes => "999.9 MB/s",
            NetUnits::Bits => "999.9 Mbps",
        },
    }
}

/// Whether a reading is in the warning colour. A threshold of 0 never warns;
/// the battery warns at or below it while not charging, the rest at or above.
pub fn warns(r: StatReading, percent: f32, threshold: f32, charging: bool) -> bool {
    if threshold <= 0.0 {
        return false;
    }
    match r {
        StatReading::Battery => percent <= threshold && !charging,
        _ => percent >= threshold,
    }
}

/// Puts the drive, as "C:", where the template says `{drive}`.
pub fn drive_name(template: &str, letter: char) -> String {
    template.replace("{drive}", &format!("{letter}:"))
}

/// Empty takes every adapter; otherwise the filter must be part of its name
/// or description, whatever the case.
pub fn adapter_matches(filter: &str, alias: &str, description: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.to_lowercase();
    alias.to_lowercase().contains(&filter) || description.to_lowercase().contains(&filter)
}

/// Empty takes every drive; otherwise the letters in the filter, ignoring
/// anything that is not a letter, such as `:` or `,`.
pub fn drive_wanted(filter: &str, letter: char) -> bool {
    filter.is_empty()
        || filter
            .chars()
            .any(|c| c.is_alphabetic() && c.eq_ignore_ascii_case(&letter))
}

/// A graphics adapter's locally unique id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Luid {
    pub high: u32,
    pub low: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuInstance {
    pub luid: Luid,
    pub phys: u32,
    pub engine: Option<u32>,
}

/// Reads a PDH GPU counter instance name, such as
/// `pid_1234_luid_0x00000000_0x0000A1B2_phys_0_eng_3_engtype_3D` or the
/// memory form `luid_0x00000000_0x0000A1B2_phys_0`.
pub fn parse_gpu_instance(name: &str) -> Option<GpuInstance> {
    let t: Vec<&str> = name.split('_').collect();
    let find = |key: &str| t.iter().position(|s| s.eq_ignore_ascii_case(key));
    let hex = |s: &str| {
        let digits = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
        u32::from_str_radix(digits, 16).ok()
    };
    let l = find("luid")?;
    let luid = Luid {
        high: hex(t.get(l + 1).copied()?)?,
        low: hex(t.get(l + 2).copied()?)?,
    };
    let phys = t.get(find("phys")? + 1)?.parse::<u32>().ok()?;
    let engine = find("eng").and_then(|e| t.get(e + 1)?.parse::<u32>().ok());
    Some(GpuInstance { luid, phys, engine })
}

/// How busy the adapter is, as Task Manager works it out: the load of each
/// engine summed over processes, then the busiest engine.
pub fn gpu_percent(samples: &[(String, f64)], luid: Luid) -> Option<f32> {
    let mut engines: HashMap<(u32, u32), f64> = HashMap::new();
    for (name, value) in samples {
        let Some(GpuInstance {
            luid: l,
            phys,
            engine: Some(engine),
        }) = parse_gpu_instance(name)
        else {
            continue;
        };
        if l == luid {
            *engines.entry((phys, engine)).or_default() += value;
        }
    }
    engines
        .values()
        .copied()
        .reduce(f64::max)
        .map(|busiest| busiest.clamp(0.0, 100.0) as f32)
}

/// Bytes of video memory in use on the adapter, summed over processes.
pub fn vram_used(samples: &[(String, f64)], luid: Luid) -> Option<u64> {
    let mut found = false;
    let mut sum = 0.0;
    for (name, value) in samples {
        if parse_gpu_instance(name).is_some_and(|i| i.luid == luid) {
            found = true;
            sum += value;
        }
    }
    found.then(|| sum.max(0.0) as u64)
}

/// The fastest rate seen, for a network meter with no fixed maximum.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Peak(f64);

impl Peak {
    /// How full a meter is, 0 to 1. With a `max` it is `rate / max`;
    /// without, the peak grows to the fastest rate seen and the rate is
    /// measured against that.
    pub fn fraction(&mut self, rate: f64, max: f64) -> f32 {
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            0.0
        };
        if max > 0.0 {
            return (rate / max).clamp(0.0, 1.0) as f32;
        }
        self.0 = self.0.max(rate);
        if self.0 <= 0.0 {
            0.0
        } else {
            (rate / self.0) as f32
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Peaks {
    pub down: Peak,
    pub up: Peak,
}

/// What the sampler has to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub interval_ms: u32,
    pub shown: Vec<StatReading>,
    pub adapter: String,
    pub drives: String,
}

impl Wanted {
    pub fn has(&self, r: StatReading) -> bool {
        self.shown.contains(&r)
    }

    /// Whether the graphics counters are needed at all.
    pub fn gpu(&self) -> bool {
        self.has(StatReading::Gpu) || self.has(StatReading::Vram)
    }

    pub fn all() -> Wanted {
        Wanted {
            interval_ms: 1000,
            shown: StatReading::ALL.to_vec(),
            adapter: String::new(),
            drives: String::new(),
        }
    }
}

impl StatsCfg {
    pub fn wanted(&self) -> Wanted {
        Wanted {
            interval_ms: self.refresh_ms,
            // In the standard order, not the display one, so moving a reading
            // does not look like a change to the sampler.
            shown: StatReading::ALL
                .into_iter()
                .filter(|&r| self.reading(r).show)
                .collect(),
            adapter: self.net_adapter.clone(),
            drives: self.drives.clone(),
        }
    }
}

/// One name and value, with how full its meter is.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub value: String,
    /// 0 to 1, in steps of 0.01 so a meter only redraws when it visibly moves.
    pub fraction: f32,
    pub warn: bool,
}

/// One reading on screen: the disks reading has a row per drive.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: StatReading,
    pub color: String,
    pub rows: Vec<Row>,
}

fn quantize(fraction: f32) -> f32 {
    if fraction.is_finite() {
        (fraction.clamp(0.0, 1.0) * 100.0).round() / 100.0
    } else {
        0.0
    }
}

fn missing(name: &str) -> Row {
    Row {
        name: name.to_owned(),
        value: "–".to_owned(),
        fraction: 0.0,
        warn: false,
    }
}

fn percent_row(id: StatReading, name: &str, percent: f32, threshold: f32) -> Row {
    Row {
        name: name.to_owned(),
        value: format_percent(percent),
        fraction: quantize(percent / 100.0),
        warn: warns(id, percent, threshold, false),
    }
}

fn usage_row(
    id: StatReading,
    name: &str,
    usage: Usage,
    format: ValueFormat,
    threshold: f32,
) -> Row {
    let percent = usage.percent();
    Row {
        name: name.to_owned(),
        value: match format {
            ValueFormat::Percent => format_percent(percent),
            ValueFormat::Amount => format_amount(usage.used, usage.total),
        },
        fraction: quantize(percent / 100.0),
        warn: warns(id, percent, threshold, false),
    }
}

/// The readings to draw, in order, for one sample.
pub fn items(cfg: &StatsCfg, sample: &Sample, peaks: &mut Peaks) -> Vec<Item> {
    let mut out = Vec::new();
    for &id in &cfg.order {
        let rd = cfg.reading(id);
        if !rd.show {
            continue;
        }
        let rows = match id {
            StatReading::Cpu => vec![sample.cpu.map_or_else(
                || missing(&rd.name),
                |p| percent_row(id, &rd.name, p, rd.warn),
            )],
            StatReading::Gpu => vec![sample.gpu.map_or_else(
                || missing(&rd.name),
                |p| percent_row(id, &rd.name, p, rd.warn),
            )],
            StatReading::Ram | StatReading::Vram => {
                let usage = if id == StatReading::Ram {
                    sample.ram
                } else {
                    sample.vram
                };
                vec![usage.map_or_else(
                    || missing(&rd.name),
                    |u| usage_row(id, &rd.name, u, rd.value, rd.warn),
                )]
            }
            StatReading::Disks => sample
                .disks
                .iter()
                .map(|d| {
                    let usage = Usage {
                        used: d.used,
                        total: d.total,
                    };
                    usage_row(
                        id,
                        &drive_name(&rd.name, d.letter),
                        usage,
                        rd.value,
                        rd.warn,
                    )
                })
                .collect(),
            StatReading::Down | StatReading::Up => {
                let (rate, peak) = if id == StatReading::Down {
                    (sample.down, &mut peaks.down)
                } else {
                    (sample.up, &mut peaks.up)
                };
                vec![rate.map_or_else(
                    || missing(&rd.name),
                    |r| {
                        let fraction = quantize(peak.fraction(r, rd.max));
                        Row {
                            name: rd.name.clone(),
                            value: format_rate(r, cfg.net_units),
                            fraction,
                            warn: warns(id, fraction * 100.0, rd.warn, false),
                        }
                    },
                )]
            }
            StatReading::Battery => sample
                .battery
                .map(|b| Row {
                    name: rd.name.clone(),
                    value: format_percent(b.percent),
                    fraction: quantize(b.percent / 100.0),
                    warn: warns(id, b.percent, rd.warn, b.charging),
                })
                .into_iter()
                .collect(),
        };
        if !rows.is_empty() {
            out.push(Item {
                id,
                color: rd.color.clone(),
                rows,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub structure: StatsStructure,
    pub orientation: StatsOrientation,
    pub columns: u32,
    pub gap: f32,
    pub padding: f32,
}

/// Where each element goes and how much room it has, plus the whole size.
#[derive(Debug, Clone, PartialEq)]
pub struct Laid {
    pub at: Vec<(f32, f32)>,
    pub cell: Vec<Size>,
    pub size: Size,
}

/// Places elements of the given sizes. Outside the free structure the first
/// one is always at `(padding, padding)`; `free` holds the positions the free
/// structure uses.
pub fn layout(grid: &Grid, sizes: &[Size], free: &[(f32, f32)]) -> Laid {
    let (p, gap) = (grid.padding, grid.gap);
    let widest = sizes.iter().fold(0.0f32, |w, s| w.max(s.w));
    let mut at = Vec::with_capacity(sizes.len());
    let cell: Vec<Size>;
    match grid.structure {
        StatsStructure::Strip => {
            cell = sizes.to_vec();
            let mut cursor = p;
            for s in sizes {
                match grid.orientation {
                    StatsOrientation::Across => {
                        at.push((cursor, p));
                        cursor += s.w + gap;
                    }
                    StatsOrientation::Down => {
                        at.push((p, cursor));
                        cursor += s.h + gap;
                    }
                }
            }
        }
        StatsStructure::List => {
            cell = sizes.iter().map(|s| Size { w: widest, h: s.h }).collect();
            let mut y = p;
            for s in sizes {
                at.push((p, y));
                y += s.h + gap;
            }
        }
        StatsStructure::Tiles => {
            let c = grid.columns.max(1) as usize;
            let slots: Vec<(usize, usize)> = (0..sizes.len())
                .map(|i| match grid.orientation {
                    StatsOrientation::Across => (i % c, i / c),
                    StatsOrientation::Down => (i / c, i % c),
                })
                .collect();
            let mut col_w: Vec<f32> = Vec::new();
            let mut row_h: Vec<f32> = Vec::new();
            for (&(col, row), s) in slots.iter().zip(sizes) {
                col_w.resize(col_w.len().max(col + 1), 0.0);
                row_h.resize(row_h.len().max(row + 1), 0.0);
                col_w[col] = col_w[col].max(s.w);
                row_h[row] = row_h[row].max(s.h);
            }
            // Each column starts where the one before it ends, plus the gap.
            let starts = |sizes: &[f32]| -> Vec<f32> {
                let mut at = p;
                sizes
                    .iter()
                    .map(|s| {
                        let start = at;
                        at += s + gap;
                        start
                    })
                    .collect()
            };
            let (col_x, row_y) = (starts(&col_w), starts(&row_h));
            cell = slots
                .iter()
                .map(|&(col, row)| Size {
                    w: col_w[col],
                    h: row_h[row],
                })
                .collect();
            at.extend(slots.iter().map(|&(col, row)| (col_x[col], row_y[row])));
        }
        StatsStructure::Free => {
            cell = sizes.to_vec();
            for i in 0..sizes.len() {
                at.push(free.get(i).copied().unwrap_or((p, p)));
            }
        }
    }
    let size = if sizes.is_empty() {
        Size {
            w: 2.0 * p,
            h: 2.0 * p,
        }
    } else {
        let (mut w, mut h) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (a, c) in at.iter().zip(&cell) {
            w = w.max(a.0 + c.w);
            h = h.max(a.1 + c.h);
        }
        Size { w: w + p, h: h + p }
    };
    Laid { at, cell, size }
}

/// The measured text of one element.
#[derive(Debug, Clone, Copy)]
pub struct Block {
    pub name: Size,
    pub value: Size,
}

#[derive(Debug, Clone, Copy)]
pub struct MeterSpec {
    pub kind: StatsMeter,
    pub length: f32,
    pub thickness: f32,
    pub ring: f32,
}

/// An element's parts, placed from its own top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Composed {
    pub size: Size,
    pub name: (f32, f32),
    pub value: (f32, f32),
    /// x, y, width, height.
    pub meter: Option<[f32; 4]>,
}

/// Space between the parts of one element.
pub const INNER_GAP: f32 = 6.0;

/// Arranges name, value and meter into one element. Side by side parts are
/// centred on the taller one. A bar goes below the text, a ring to its left.
pub fn compose(block: Block, names: StatsNames, meter: MeterSpec) -> Composed {
    let (n, v) = (block.name, block.value);
    let (text, name, value) = match names {
        StatsNames::Hidden => (v, (0.0, 0.0), (0.0, 0.0)),
        StatsNames::Above => (
            Size {
                w: n.w.max(v.w),
                h: n.h + v.h,
            },
            (0.0, 0.0),
            (0.0, n.h),
        ),
        StatsNames::Before | StatsNames::After => {
            let h = n.h.max(v.h);
            let (first, second) = if names == StatsNames::Before {
                (n, v)
            } else {
                (v, n)
            };
            let second_x = first.w + INNER_GAP;
            let first_pos = (0.0, (h - first.h) / 2.0);
            let second_pos = (second_x, (h - second.h) / 2.0);
            let (name, value) = if names == StatsNames::Before {
                (first_pos, second_pos)
            } else {
                (second_pos, first_pos)
            };
            (
                Size {
                    w: second_x + second.w,
                    h,
                },
                name,
                value,
            )
        }
    };
    match meter.kind {
        StatsMeter::None => Composed {
            size: text,
            name,
            value,
            meter: None,
        },
        StatsMeter::Bar => {
            let length = if meter.length > 0.0 {
                meter.length
            } else {
                text.w
            };
            Composed {
                size: Size {
                    w: text.w.max(length),
                    h: text.h + INNER_GAP + meter.thickness,
                },
                name,
                value,
                meter: Some([0.0, text.h + INNER_GAP, length, meter.thickness]),
            }
        }
        StatsMeter::Ring => {
            let ring = meter.ring;
            let h = ring.max(text.h);
            let (dx, dy) = (ring + INNER_GAP, (h - text.h) / 2.0);
            Composed {
                size: Size { w: dx + text.w, h },
                name: (name.0 + dx, name.1 + dy),
                value: (value.0 + dx, value.1 + dy),
                meter: Some([0.0, (h - ring) / 2.0, ring, ring]),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: f64 = 1_073_741_824.0;

    fn times(idle: u64, kernel: u64, user: u64) -> CpuTimes {
        CpuTimes { idle, kernel, user }
    }

    #[test]
    fn a_new_or_reset_adapter_adds_no_spike() {
        let prev = [(1, 100, 10), (2, 500, 50)];
        let now = [(1, 150, 30), (2, 40, 60), (3, 9_000_000, 9_000_000)];
        assert_eq!(octet_deltas(&prev, &now), (50, 30));
    }

    #[test]
    fn moving_a_reading_does_not_change_what_is_wanted() {
        let mut cfg = StatsCfg::default();
        let before = cfg.wanted();
        cfg.move_reading(StatReading::Cpu, 2);
        assert_eq!(cfg.wanted(), before);
    }

    #[test]
    fn usage_is_a_clamped_percent() {
        assert_eq!(Usage::default().percent(), 0.0);
        assert_eq!(Usage { used: 1, total: 4 }.percent(), 25.0);
        assert_eq!(Usage { used: 9, total: 4 }.percent(), 100.0);
    }

    #[test]
    fn cpu_load_comes_from_the_change_in_times() {
        let prev = times(100, 200, 100);
        assert_eq!(cpu_percent(prev, times(200, 300, 200)), Some(50.0));
        assert_eq!(cpu_percent(prev, prev), None);
        assert_eq!(cpu_percent(prev, times(50, 300, 200)), None);
    }

    #[test]
    fn a_reset_counter_has_no_rate() {
        assert_eq!(byte_rate(1000, 3000, 2.0), 1000.0);
        assert_eq!(byte_rate(3000, 1000, 1.0), 0.0);
        assert_eq!(byte_rate(0, 1000, 0.0), 0.0);
    }

    #[test]
    fn rates_step_up_at_a_thousand_before_rounding() {
        let bytes = |v| format_rate(v, NetUnits::Bytes);
        assert_eq!(bytes(0.0), "0.0 KB/s");
        assert_eq!(bytes(999.0), "1.0 KB/s");
        // 999.9 KB/s has not reached 1000 yet, so it stays in KB/s.
        assert_eq!(bytes(999_900.0), "999.9 KB/s");
        assert_eq!(bytes(1_000_000.0), "1.0 MB/s");
        assert_eq!(bytes(12_300_000.0), "12.3 MB/s");
        assert_eq!(bytes(1_200_000_000.0), "1.2 GB/s");
        assert_eq!(bytes(5e12), "5000.0 GB/s");
        assert_eq!(bytes(f64::NAN), "0.0 KB/s");
        assert_eq!(bytes(-5.0), "0.0 KB/s");

        let bits = |v| format_rate(v, NetUnits::Bits);
        assert_eq!(bits(0.0), "0.0 Kbps");
        assert_eq!(bits(12_500.0), "100.0 Kbps");
        assert_eq!(bits(125_000.0), "1.0 Mbps");
        assert_eq!(bits(12_300_000.0), "98.4 Mbps");
        assert_eq!(bits(125_000_000.0), "1.0 Gbps");
    }

    #[test]
    fn amounts_take_their_unit_from_the_total() {
        let at = |used: f64, total: f64| format_amount((used * GIB) as u64, (total * GIB) as u64);
        assert_eq!(at(9.3, 16.0), "9.3 / 16 GB");
        assert_eq!(at(0.9 * 1024.0, 1.8 * 1024.0), "0.9 / 1.8 TB");
        assert_eq!(at(0.25, 0.5), "256 / 512 MB");
        assert_eq!(format_amount(5, 0), "–");
    }

    #[test]
    fn percent_is_whole_and_clamped() {
        assert_eq!(format_percent(57.6), "58%");
        assert_eq!(format_percent(250.0), "100%");
        assert_eq!(format_percent(f32::NAN), "0%");
    }

    #[test]
    fn the_widest_value_depends_on_the_format() {
        use StatReading::*;
        let w = |r, f, u| widest_value(r, f, u);
        assert_eq!(w(Cpu, ValueFormat::Amount, NetUnits::Bytes), "100%");
        assert_eq!(w(Ram, ValueFormat::Percent, NetUnits::Bytes), "100%");
        assert_eq!(
            w(Disks, ValueFormat::Amount, NetUnits::Bytes),
            "99.9 / 99.9 GB"
        );
        assert_eq!(w(Down, ValueFormat::Percent, NetUnits::Bytes), "999.9 MB/s");
        assert_eq!(w(Up, ValueFormat::Percent, NetUnits::Bits), "999.9 Mbps");
        assert_eq!(w(Battery, ValueFormat::Percent, NetUnits::Bits), "100%");
    }

    #[test]
    fn each_reading_warns_its_own_way() {
        use StatReading::*;
        assert!(warns(Cpu, 90.0, 90.0, false));
        assert!(!warns(Cpu, 89.0, 90.0, false));
        assert!(!warns(Ram, 100.0, 0.0, false));
        assert!(warns(Down, 100.0, 50.0, false));
        assert!(warns(Battery, 20.0, 20.0, false));
        assert!(!warns(Battery, 21.0, 20.0, false));
        assert!(!warns(Battery, 5.0, 20.0, true));
    }

    #[test]
    fn filters_and_names() {
        assert_eq!(drive_name("{drive}", 'C'), "C:");
        assert_eq!(drive_name("Disk {drive} used", 'D'), "Disk D: used");
        assert_eq!(drive_name("Plain", 'D'), "Plain");
        assert!(adapter_matches("", "Wi-Fi", "Intel"));
        assert!(adapter_matches("wi-fi", "Wi-Fi", "Intel"));
        assert!(adapter_matches("INTEL", "Wi-Fi", "Intel(R) Wi-Fi 6"));
        assert!(!adapter_matches("ethernet", "Wi-Fi", "Intel"));
        assert!(drive_wanted("", 'C'));
        assert!(drive_wanted("c, d:", 'C'));
        assert!(drive_wanted("c, d:", 'D'));
        assert!(!drive_wanted("c, d:", 'E'));
        assert!(!drive_wanted(":,", 'C'));
    }

    const LUID: Luid = Luid {
        high: 0,
        low: 0xA1B2,
    };

    #[test]
    fn gpu_instance_names_are_parsed() {
        let full = "pid_1234_luid_0x00000000_0x0000A1B2_phys_0_eng_3_engtype_3D";
        assert_eq!(
            parse_gpu_instance(full),
            Some(GpuInstance {
                luid: LUID,
                phys: 0,
                engine: Some(3)
            })
        );
        assert_eq!(
            parse_gpu_instance("luid_0x00000000_0x0000a1b2_phys_1"),
            Some(GpuInstance {
                luid: LUID,
                phys: 1,
                engine: None
            })
        );
        assert_eq!(parse_gpu_instance("garbage"), None);
        assert_eq!(parse_gpu_instance("luid_0xZZ_0x1_phys_0"), None);
        assert_eq!(parse_gpu_instance("luid_0x0_0x1"), None);
    }

    #[test]
    fn gpu_load_is_the_busiest_engine_summed_over_processes() {
        let s = |name: &str, v: f64| (name.to_owned(), v);
        let samples = [
            s(
                "pid_1_luid_0x00000000_0x0000A1B2_phys_0_eng_3_engtype_3D",
                10.0,
            ),
            s(
                "pid_2_luid_0x00000000_0x0000A1B2_phys_0_eng_3_engtype_3D",
                15.0,
            ),
            s(
                "pid_1_luid_0x00000000_0x0000A1B2_phys_0_eng_0_engtype_Copy",
                5.0,
            ),
            s(
                "pid_1_luid_0x00000000_0x0000FFFF_phys_0_eng_3_engtype_3D",
                99.0,
            ),
        ];
        assert_eq!(gpu_percent(&samples, LUID), Some(25.0));
        let other = Luid { high: 0, low: 7 };
        assert_eq!(gpu_percent(&samples, other), None);
        let over = [s(
            "pid_1_luid_0x00000000_0x0000A1B2_phys_0_eng_0_engtype_3D",
            140.0,
        )];
        assert_eq!(gpu_percent(&over, LUID), Some(100.0));
    }

    #[test]
    fn video_memory_is_summed_for_one_adapter() {
        let s = |name: &str, v: f64| (name.to_owned(), v);
        let samples = [
            s("luid_0x00000000_0x0000A1B2_phys_0", 1000.0),
            s("pid_9_luid_0x00000000_0x0000A1B2_phys_0", 500.0),
            s("luid_0x00000000_0x0000FFFF_phys_0", 7000.0),
        ];
        assert_eq!(vram_used(&samples, LUID), Some(1500));
        assert_eq!(vram_used(&samples, Luid { high: 1, low: 1 }), None);
    }

    #[test]
    fn a_peak_grows_unless_a_maximum_is_set() {
        let mut fixed = Peak::default();
        assert_eq!(fixed.fraction(50.0, 200.0), 0.25);
        assert_eq!(fixed.fraction(500.0, 200.0), 1.0);

        let mut growing = Peak::default();
        assert_eq!(growing.fraction(0.0, 0.0), 0.0);
        assert_eq!(growing.fraction(10.0, 0.0), 1.0);
        assert_eq!(growing.fraction(5.0, 0.0), 0.5);
        assert_eq!(growing.fraction(f64::NAN, 0.0), 0.0);
    }

    #[test]
    fn only_shown_readings_are_wanted() {
        let mut cfg = StatsCfg::default();
        assert_eq!(cfg.wanted().shown.len(), 8);
        assert_eq!(cfg.wanted().interval_ms, 1000);
        cfg.ram.show = false;
        cfg.gpu.show = false;
        let wanted = cfg.wanted();
        assert!(!wanted.has(StatReading::Ram));
        assert!(wanted.has(StatReading::Cpu));
        assert!(wanted.gpu());
        cfg.vram.show = false;
        assert!(!cfg.wanted().gpu());
        assert!(Wanted::all().gpu());
    }

    fn sample() -> Sample {
        Sample {
            cpu: Some(58.4),
            ram: Some(Usage {
                used: (9.3 * GIB) as u64,
                total: (16.0 * GIB) as u64,
            }),
            gpu: None,
            vram: None,
            disks: vec![
                Disk {
                    letter: 'C',
                    used: (100.0 * GIB) as u64,
                    total: (200.0 * GIB) as u64,
                },
                Disk {
                    letter: 'D',
                    used: (190.0 * GIB) as u64,
                    total: (200.0 * GIB) as u64,
                },
            ],
            down: Some(12_300_000.0),
            up: Some(0.0),
            battery: Some(Battery {
                percent: 15.0,
                charging: true,
            }),
        }
    }

    fn item(items: &[Item], id: StatReading) -> Option<&Item> {
        items.iter().find(|i| i.id == id)
    }

    #[test]
    fn a_sample_becomes_rows() {
        let mut cfg = StatsCfg::default();
        let mut peaks = Peaks::default();
        let out = items(&cfg, &sample(), &mut peaks);
        assert_eq!(out.len(), 8);

        let cpu = &item(&out, StatReading::Cpu).unwrap().rows[0];
        assert_eq!((cpu.name.as_str(), cpu.value.as_str()), ("CPU", "58%"));
        assert_eq!(cpu.fraction, 0.58);
        assert!(!cpu.warn);

        assert_eq!(item(&out, StatReading::Ram).unwrap().rows[0].value, "58%");
        let gpu = &item(&out, StatReading::Gpu).unwrap().rows[0];
        assert_eq!(
            (gpu.value.as_str(), gpu.fraction, gpu.warn),
            ("–", 0.0, false)
        );

        let disks = &item(&out, StatReading::Disks).unwrap().rows;
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].name, "C:");
        assert_eq!(disks[1].name, "D:");
        assert!(!disks[0].warn);
        assert!(disks[1].warn);

        let down = &item(&out, StatReading::Down).unwrap().rows[0];
        assert_eq!(down.value, "12.3 MB/s");
        assert_eq!(down.fraction, 1.0);
        assert!(!down.warn);

        // Charging, so the low battery is not a warning.
        let battery = &item(&out, StatReading::Battery).unwrap().rows[0];
        assert_eq!(battery.value, "15%");
        assert!(!battery.warn);

        cfg.ram.value = ValueFormat::Amount;
        cfg.net_units = NetUnits::Bits;
        let mut unplugged = sample();
        unplugged.battery = Some(Battery {
            percent: 15.0,
            charging: false,
        });
        unplugged.cpu = Some(95.0);
        let out = items(&cfg, &unplugged, &mut Peaks::default());
        assert_eq!(
            item(&out, StatReading::Ram).unwrap().rows[0].value,
            "9.3 / 16 GB"
        );
        assert_eq!(
            item(&out, StatReading::Down).unwrap().rows[0].value,
            "98.4 Mbps"
        );
        assert!(item(&out, StatReading::Battery).unwrap().rows[0].warn);
        assert!(item(&out, StatReading::Cpu).unwrap().rows[0].warn);
    }

    #[test]
    fn missing_hardware_leaves_its_reading_out() {
        let mut s = sample();
        s.battery = None;
        s.disks.clear();
        let out = items(&StatsCfg::default(), &s, &mut Peaks::default());
        assert!(item(&out, StatReading::Battery).is_none());
        assert!(item(&out, StatReading::Disks).is_none());
        assert_eq!(out.len(), 6);

        let mut cfg = StatsCfg::default();
        cfg.cpu.show = false;
        cfg.cpu.color = "accent".to_owned();
        cfg.ram.color = "#FF0000".to_owned();
        let out = items(&cfg, &sample(), &mut Peaks::default());
        assert!(item(&out, StatReading::Cpu).is_none());
        assert_eq!(item(&out, StatReading::Ram).unwrap().color, "#FF0000");
    }

    fn grid(structure: StatsStructure, orientation: StatsOrientation) -> Grid {
        Grid {
            structure,
            orientation,
            columns: 3,
            gap: 2.0,
            padding: 1.0,
        }
    }

    fn sz(w: f32, h: f32) -> Size {
        Size { w, h }
    }

    #[test]
    fn a_strip_runs_across_or_down() {
        let sizes = [sz(10.0, 4.0), sz(20.0, 6.0), sz(30.0, 8.0)];
        let across = layout(
            &grid(StatsStructure::Strip, StatsOrientation::Across),
            &sizes,
            &[],
        );
        assert_eq!(across.at, [(1.0, 1.0), (13.0, 1.0), (35.0, 1.0)]);
        assert_eq!(across.cell, sizes);
        assert_eq!(across.size, sz(66.0, 10.0));

        let down = layout(
            &grid(StatsStructure::Strip, StatsOrientation::Down),
            &sizes,
            &[],
        );
        assert_eq!(down.at, [(1.0, 1.0), (1.0, 7.0), (1.0, 15.0)]);
        assert_eq!(down.size, sz(32.0, 24.0));
    }

    #[test]
    fn a_list_is_one_column_as_wide_as_its_widest() {
        let sizes = [sz(10.0, 4.0), sz(20.0, 6.0), sz(30.0, 8.0)];
        for orientation in [StatsOrientation::Across, StatsOrientation::Down] {
            let list = layout(&grid(StatsStructure::List, orientation), &sizes, &[]);
            assert_eq!(list.at, [(1.0, 1.0), (1.0, 7.0), (1.0, 15.0)]);
            assert_eq!(list.cell, [sz(30.0, 4.0), sz(30.0, 6.0), sz(30.0, 8.0)]);
            assert_eq!(list.size, sz(32.0, 24.0));
        }
    }

    #[test]
    fn tiles_fill_rows_or_columns_first() {
        let mut sizes = vec![sz(10.0, 4.0); 7];
        sizes[0] = sz(12.0, 5.0);
        let across = layout(
            &grid(StatsStructure::Tiles, StatsOrientation::Across),
            &sizes,
            &[],
        );
        assert_eq!(across.cell[1], sz(10.0, 5.0));
        assert_eq!(across.cell[3], sz(12.0, 4.0));
        assert_eq!(across.at[3], (1.0, 8.0));
        assert_eq!(across.at[6], (1.0, 14.0));
        assert_eq!(across.size, sz(38.0, 19.0));

        let down = layout(
            &grid(StatsStructure::Tiles, StatsOrientation::Down),
            &sizes,
            &[],
        );
        assert_eq!(down.at[3], (15.0, 1.0));
        assert_eq!(down.at[6], (27.0, 1.0));
        assert_eq!(down.size, sz(38.0, 19.0));
    }

    #[test]
    fn a_tall_tile_only_lengthens_its_own_row() {
        let mut sizes = vec![sz(10.0, 4.0); 6];
        sizes[3] = sz(10.0, 20.0);
        let laid = layout(
            &grid(StatsStructure::Tiles, StatsOrientation::Across),
            &sizes,
            &[],
        );
        assert_eq!(laid.cell[0], sz(10.0, 4.0));
        assert_eq!(laid.cell[5], sz(10.0, 20.0));
        assert_eq!(laid.at[3], (1.0, 7.0));
        assert_eq!(laid.size, sz(36.0, 28.0));
    }

    #[test]
    fn the_free_structure_uses_the_given_spots() {
        let sizes = [sz(10.0, 4.0), sz(20.0, 6.0)];
        let free = layout(
            &grid(StatsStructure::Free, StatsOrientation::Across),
            &sizes,
            &[(5.0, 6.0)],
        );
        assert_eq!(free.at, [(5.0, 6.0), (1.0, 1.0)]);
        assert_eq!(free.cell, sizes);
        assert_eq!(free.size, sz(22.0, 11.0));
    }

    #[test]
    fn nothing_and_one_thing_lay_out() {
        let g = grid(StatsStructure::Strip, StatsOrientation::Across);
        let none = layout(&g, &[], &[]);
        assert!(none.at.is_empty());
        assert_eq!(none.size, sz(2.0, 2.0));
        let one = layout(&g, &[sz(10.0, 4.0)], &[]);
        assert_eq!(one.at, [(1.0, 1.0)]);
        assert_eq!(one.size, sz(12.0, 6.0));
    }

    #[test]
    fn the_first_element_sits_at_the_padding_in_every_fixed_structure() {
        let sizes = [sz(10.0, 4.0), sz(20.0, 6.0), sz(30.0, 8.0)];
        for structure in [
            StatsStructure::Strip,
            StatsStructure::List,
            StatsStructure::Tiles,
        ] {
            for orientation in [StatsOrientation::Across, StatsOrientation::Down] {
                let laid = layout(&grid(structure, orientation), &sizes, &[]);
                assert_eq!(laid.at[0], (1.0, 1.0), "{structure:?} {orientation:?}");
            }
        }
    }

    const BLOCK: Block = Block {
        name: Size { w: 10.0, h: 6.0 },
        value: Size { w: 20.0, h: 10.0 },
    };

    fn plain() -> MeterSpec {
        MeterSpec {
            kind: StatsMeter::None,
            length: 0.0,
            thickness: 4.0,
            ring: 30.0,
        }
    }

    #[test]
    fn name_and_value_sit_side_by_side_or_stacked() {
        let before = compose(BLOCK, StatsNames::Before, plain());
        assert_eq!(
            before,
            Composed {
                size: sz(36.0, 10.0),
                name: (0.0, 2.0),
                value: (16.0, 0.0),
                meter: None
            }
        );
        let after = compose(BLOCK, StatsNames::After, plain());
        assert_eq!(
            after,
            Composed {
                size: sz(36.0, 10.0),
                name: (26.0, 2.0),
                value: (0.0, 0.0),
                meter: None
            }
        );
        let above = compose(BLOCK, StatsNames::Above, plain());
        assert_eq!(
            above,
            Composed {
                size: sz(20.0, 16.0),
                name: (0.0, 0.0),
                value: (0.0, 6.0),
                meter: None
            }
        );
        let hidden = compose(BLOCK, StatsNames::Hidden, plain());
        assert_eq!(hidden.size, sz(20.0, 10.0));
        assert_eq!(hidden.value, (0.0, 0.0));
    }

    #[test]
    fn a_bar_goes_below_the_text() {
        let bar = |length| MeterSpec {
            kind: StatsMeter::Bar,
            length,
            ..plain()
        };
        let own = compose(BLOCK, StatsNames::Before, bar(0.0));
        assert_eq!(own.meter, Some([0.0, 16.0, 36.0, 4.0]));
        assert_eq!(own.size, sz(36.0, 20.0));
        let long = compose(BLOCK, StatsNames::Before, bar(50.0));
        assert_eq!(long.meter, Some([0.0, 16.0, 50.0, 4.0]));
        assert_eq!(long.size, sz(50.0, 20.0));
    }

    #[test]
    fn a_ring_goes_left_of_the_text_and_centres_against_it() {
        let ring = |ring| MeterSpec {
            kind: StatsMeter::Ring,
            ring,
            ..plain()
        };
        let big = compose(BLOCK, StatsNames::Before, ring(30.0));
        assert_eq!(
            big,
            Composed {
                size: sz(72.0, 30.0),
                name: (36.0, 12.0),
                value: (52.0, 10.0),
                meter: Some([0.0, 0.0, 30.0, 30.0])
            }
        );
        let small = compose(BLOCK, StatsNames::Before, ring(4.0));
        assert_eq!(small.size, sz(46.0, 10.0));
        assert_eq!(small.meter, Some([0.0, 3.0, 4.0, 4.0]));
        assert_eq!(small.value, (26.0, 0.0));
    }
}
