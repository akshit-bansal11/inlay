//! The System widget's logic: what is read, how it is worked out and
//! written, and where each reading goes. No Windows types: the sampler in the
//! app crate fills a [`Sample`] and this turns it into text and positions.

use std::collections::HashMap;

use crate::config::{NetUnits, StatReading, StatsCfg, ValueFormat};

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
            shown: self
                .order
                .iter()
                .copied()
                .filter(|&r| self.reading(r).show)
                .collect(),
            adapter: self.net_adapter.clone(),
            drives: self.drives.clone(),
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
}
