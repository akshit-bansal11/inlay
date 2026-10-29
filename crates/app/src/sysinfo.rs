//! Reads what the System widget shows: processor, memory, graphics, drives,
//! network and battery. Every reading that fails is `None`, with one log line
//! per kind of reading rather than one per tick.

use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inlay_core::config::StatReading;
use inlay_core::stats::{self, Battery, CpuTimes, Disk, Luid, Sample, Usage, Wanted};
use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
    PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::{HSTRING, PCWSTR, w};

use crate::window::WM_APP_STATS;
use crate::{Notify, log};

/// How long drive and battery readings are kept before they are read again.
const SLOW: Duration = Duration::from_secs(30);
/// `GetDriveTypeW` results, which the `windows` crate keeps behind a feature
/// this app does not otherwise need.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
/// `MIB_IF_ROW2::InterfaceAndOperStatusFlags` bits.
const HARDWARE_INTERFACE: u8 = 1;
const FILTER_INTERFACE: u8 = 2;

/// Stops Windows asking "insert a disk" when a card reader has no card in it.
/// Called once by the thread that samples.
pub fn init_thread() {
    // SAFETY: changes only this thread's error mode; the old mode is not wanted.
    if let Err(error) = unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, None) } {
        log(&format!("system widget: error mode not set: {error}"));
    }
}

/// What the app tells the sampling thread.
pub enum Control {
    /// Read these, starting from fresh baselines.
    Run(Wanted),
    /// Read nothing until told otherwise.
    Pause,
}

/// How soon after a start the first rates are worked out.
const FIRST: Duration = Duration::from_millis(250);

/// Starts the sampling thread. It posts `WM_APP_STATS` after storing each
/// sample in `shared`, and posts nothing while paused.
pub fn spawn(shared: Arc<Mutex<Sample>>, notify: Notify) -> Sender<Control> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        init_thread();
        // The sampler holds a handle that cannot leave the thread that made
        // it, so it is built here, and again on every start, which also keeps
        // the time spent paused out of the first rates.
        let mut running: Option<(Wanted, Sampler, Instant)> = None;
        loop {
            let message = match &running {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some((_, _, next)) => {
                    rx.recv_timeout(next.saturating_duration_since(Instant::now()))
                }
            };
            match message {
                Ok(Control::Run(wanted)) => {
                    let sampler = Sampler::new(&wanted);
                    running = Some((wanted, sampler, Instant::now() + FIRST));
                }
                Ok(Control::Pause) => {
                    running = None;
                    if let Ok(mut guard) = shared.lock() {
                        *guard = Sample::default();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some((wanted, sampler, next)) = &mut running {
                        let sample = sampler.sample(wanted);
                        if let Ok(mut guard) = shared.lock() {
                            *guard = sample;
                        }
                        *next =
                            Instant::now() + Duration::from_millis(u64::from(wanted.interval_ms));
                        notify.post(WM_APP_STATS);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

/// Keeps what one reading needs from the one before: the processor and
/// network counters for rates, the graphics query, the slow readings.
pub struct Sampler {
    cpu: Option<CpuTimes>,
    net: Option<Net>,
    gpu: Option<Gpu>,
    /// Opening the graphics query failed; not retried until graphics is off and on again.
    gpu_failed: bool,
    slow: Option<Slow>,
    /// One bit per [`StatReading`] already logged as failing.
    logged: u8,
}

struct Net {
    /// (interface LUID, octets in, octets out) per adapter counted.
    rows: Vec<(u64, u64, u64)>,
    at: Instant,
    adapter: String,
}

struct Slow {
    at: Instant,
    drives: String,
    wants: (bool, bool),
    disks: Vec<Disk>,
    battery: Option<Battery>,
}

impl Sampler {
    /// Takes the first reading, so the next one has rates to work out.
    pub fn new(wanted: &Wanted) -> Sampler {
        let mut sampler = Sampler {
            cpu: None,
            net: None,
            gpu: None,
            gpu_failed: false,
            slow: None,
            logged: 0,
        };
        sampler.sample(wanted);
        sampler
    }

    pub fn sample(&mut self, wanted: &Wanted) -> Sample {
        let mut sample = Sample::default();

        if wanted.has(StatReading::Cpu) {
            let now = cpu_times();
            if now.is_none() {
                self.fail(StatReading::Cpu, "processor times not read");
            }
            sample.cpu = self
                .cpu
                .zip(now)
                .and_then(|(prev, now)| stats::cpu_percent(prev, now));
            self.cpu = now;
        } else {
            self.cpu = None;
        }

        if wanted.has(StatReading::Ram) {
            sample.ram = memory();
            if sample.ram.is_none() {
                self.fail(StatReading::Ram, "memory status not read");
            }
        }

        if wanted.gpu() {
            if self.gpu.is_none() && !self.gpu_failed {
                match Gpu::open() {
                    Ok(gpu) => self.gpu = Some(gpu),
                    Err(what) => {
                        self.gpu_failed = true;
                        self.fail(StatReading::Gpu, &what);
                    }
                }
            }
            if let Some(gpu) = &self.gpu {
                // SAFETY: the query is open until `gpu` is dropped.
                if unsafe { PdhCollectQueryData(gpu.query) } == 0 {
                    if wanted.has(StatReading::Gpu) {
                        sample.gpu = counter_values(gpu.engine)
                            .and_then(|items| stats::gpu_percent(&items, gpu.luid));
                    }
                    if wanted.has(StatReading::Vram) {
                        sample.vram = counter_values(gpu.memory)
                            .and_then(|items| stats::vram_used(&items, gpu.luid))
                            .map(|used| Usage {
                                used,
                                total: gpu.dedicated,
                            });
                    }
                }
            }
        } else {
            self.gpu = None;
            self.gpu_failed = false;
        }

        if wanted.has(StatReading::Down) || wanted.has(StatReading::Up) {
            let now = net_octets(&wanted.adapter);
            if now.is_none() {
                self.fail(StatReading::Down, "network table not read");
            }
            let at = Instant::now();
            if let (Some(prev), Some(rows)) = (&self.net, &now)
                && prev.adapter == wanted.adapter
            {
                let secs = at.duration_since(prev.at).as_secs_f64();
                let (down, up) = stats::octet_deltas(&prev.rows, rows);
                if wanted.has(StatReading::Down) {
                    sample.down = Some(stats::byte_rate(0, down, secs));
                }
                if wanted.has(StatReading::Up) {
                    sample.up = Some(stats::byte_rate(0, up, secs));
                }
            }
            self.net = now.map(|rows| Net {
                rows,
                at,
                adapter: wanted.adapter.clone(),
            });
        } else {
            self.net = None;
        }

        let wants = (
            wanted.has(StatReading::Disks),
            wanted.has(StatReading::Battery),
        );
        if wants.0 || wants.1 {
            let stale = self.slow.as_ref().is_none_or(|slow| {
                slow.at.elapsed() >= SLOW || slow.drives != wanted.drives || slow.wants != wants
            });
            if stale {
                let disks = if wants.0 {
                    self.disks(&wanted.drives)
                } else {
                    Vec::new()
                };
                let battery = if wants.1 { self.battery() } else { None };
                self.slow = Some(Slow {
                    at: Instant::now(),
                    drives: wanted.drives.clone(),
                    wants,
                    disks,
                    battery,
                });
            }
            if let Some(slow) = &self.slow {
                sample.disks = slow.disks.clone();
                sample.battery = slow.battery;
            }
        } else {
            self.slow = None;
        }

        sample
    }

    fn fail(&mut self, reading: StatReading, what: &str) {
        let bit = 1u8 << (reading as u8);
        if self.logged & bit == 0 {
            self.logged |= bit;
            log(&format!("system widget: {what}"));
        }
    }

    /// Fixed and removable drives the filter takes, skipping any that cannot
    /// be read, such as a card reader with no card.
    fn disks(&mut self, filter: &str) -> Vec<Disk> {
        // SAFETY: takes no arguments.
        let mask = unsafe { GetLogicalDrives() };
        if mask == 0 {
            self.fail(StatReading::Disks, "drive list not read");
        }
        (0..26u8)
            .filter(|i| mask & (1u32 << i) != 0)
            .map(|i| char::from(b'A' + i))
            .filter(|&letter| stats::drive_wanted(filter, letter))
            .filter_map(|letter| {
                let root = HSTRING::from(format!("{letter}:\\"));
                // SAFETY: `root` is NUL-terminated and outlives the call.
                let kind = unsafe { GetDriveTypeW(&root) };
                if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
                    return None;
                }
                let (mut total, mut free) = (0u64, 0u64);
                // SAFETY: `root` outlives the call; both outputs are live locals.
                unsafe {
                    GetDiskFreeSpaceExW(&root, None, Some(&raw mut total), Some(&raw mut free))
                }
                .ok()?;
                (total > 0).then(|| Disk {
                    letter,
                    used: total.saturating_sub(free),
                    total,
                })
            })
            .collect()
    }

    /// `None` on a PC with no battery, or one Windows cannot read.
    fn battery(&mut self) -> Option<Battery> {
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: `status` is a live local the call fills.
        if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
            self.fail(StatReading::Battery, "power status not read");
            return None;
        }
        // 128 is "no system battery", 255 "unknown".
        if status.BatteryFlag == 255
            || status.BatteryFlag & 128 != 0
            || status.BatteryLifePercent == 255
        {
            return None;
        }
        Some(Battery {
            percent: f32::from(status.BatteryLifePercent),
            charging: status.BatteryFlag & 8 != 0,
        })
    }
}

fn cpu_times() -> Option<CpuTimes> {
    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the three outputs are live locals.
    unsafe {
        GetSystemTimes(
            Some(&raw mut idle),
            Some(&raw mut kernel),
            Some(&raw mut user),
        )
    }
    .ok()?;
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Some(CpuTimes {
        idle: ticks(idle),
        kernel: ticks(kernel),
        user: ticks(user),
    })
}

fn memory() -> Option<Usage> {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` is a live local with `dwLength` set, as the call requires.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(Usage {
        used: status.ullTotalPhys.saturating_sub(status.ullAvailPhys),
        total: status.ullTotalPhys,
    })
}

/// Octets in and out, per adapter, of the network adapters that are up and
/// match the filter: hardware ones, not their filter layers or loopback.
fn net_octets(filter: &str) -> Option<Vec<(u64, u64, u64)>> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: `table` is a live local the call points at memory it allocates.
    if unsafe { GetIfTable2(&mut table) } != ERROR_SUCCESS || table.is_null() {
        return None;
    }
    // SAFETY: on success `table` holds `NumEntries` rows, valid until FreeMibTable.
    let rows = unsafe {
        std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize)
    };
    let mut found = Vec::new();
    for row in rows {
        let flags = row.InterfaceAndOperStatusFlags._bitfield;
        if row.OperStatus == IfOperStatusUp
            && flags & HARDWARE_INTERFACE != 0
            && flags & FILTER_INTERFACE == 0
            && row.Type != IF_TYPE_SOFTWARE_LOOPBACK
            && stats::adapter_matches(filter, &wide(&row.Alias), &wide(&row.Description))
        {
            // SAFETY: `Value` is the u64 view of the LUID union, always valid.
            let luid = unsafe { row.InterfaceLuid.Value };
            found.push((luid, row.InOctets, row.OutOctets));
        }
    }
    // SAFETY: `table` came from GetIfTable2, is freed once, and `rows` is not used after.
    unsafe { FreeMibTable(table.cast_const().cast()) };
    Some(found)
}

/// A NUL-terminated UTF-16 buffer as text.
fn wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// The graphics counters, for the adapter with the most video memory of its
/// own, the way Task Manager reads them.
struct Gpu {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    memory: PDH_HCOUNTER,
    luid: Luid,
    dedicated: u64,
}

impl Gpu {
    fn open() -> Result<Gpu, String> {
        let (luid, dedicated) = biggest_adapter().ok_or("no graphics adapter found")?;
        let mut query = PDH_HQUERY::default();
        // SAFETY: `query` is a live local the call fills; no data source is named.
        let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if status != 0 {
            return Err(format!("graphics query not opened: {status:#x}"));
        }
        // From here `Drop` closes the query, whatever fails next.
        let mut gpu = Gpu {
            query,
            engine: PDH_HCOUNTER::default(),
            memory: PDH_HCOUNTER::default(),
            luid,
            dedicated,
        };
        // SAFETY: the query is open; the paths are NUL-terminated literals; the
        // outputs are fields of `gpu`.
        let status = unsafe {
            match PdhAddEnglishCounterW(
                gpu.query,
                w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut gpu.engine,
            ) {
                0 => PdhAddEnglishCounterW(
                    gpu.query,
                    w!("\\GPU Adapter Memory(*)\\Dedicated Usage"),
                    0,
                    &mut gpu.memory,
                ),
                status => status,
            }
        };
        if status != 0 {
            return Err(format!("graphics counters not added: {status:#x}"));
        }
        Ok(gpu)
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // SAFETY: the query was opened by `Gpu::open` and is closed only here.
        unsafe { PdhCloseQuery(self.query) };
    }
}

fn biggest_adapter() -> Option<(Luid, u64)> {
    // SAFETY: creates a factory; no arguments.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    (0..)
        // SAFETY: enumeration stops at the first index with no adapter.
        .map_while(|i| unsafe { factory.EnumAdapters1(i) }.ok())
        // SAFETY: the adapter is a live interface.
        .filter_map(|adapter| unsafe { adapter.GetDesc1() }.ok())
        .max_by_key(|desc| desc.DedicatedVideoMemory)
        .map(|desc| {
            let luid = Luid {
                high: desc.AdapterLuid.HighPart as u32,
                low: desc.AdapterLuid.LowPart,
            };
            (luid, desc.DedicatedVideoMemory as u64)
        })
}

/// Every instance of a wildcard counter, by name, from the last collect.
/// `None` until a second collect has given rates something to compare.
fn counter_values(counter: PDH_HCOUNTER) -> Option<Vec<(String, f64)>> {
    let (mut size, mut count) = (0u32, 0u32);
    // SAFETY: with no buffer the call only writes the size it needs.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
    };
    if status != PDH_MORE_DATA {
        return None;
    }
    let item = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); (size as usize).div_ceil(item)];
    let mut size = (items.len() * item) as u32;
    // SAFETY: `items` holds `size` bytes; the names it gets point into it.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(items.as_mut_ptr()),
        )
    };
    if status != 0 {
        return None;
    }
    let values = items
        .iter()
        .take(count as usize)
        .filter(|item| {
            item.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA
                || item.FmtValue.CStatus == PDH_CSTATUS_NEW_DATA
        })
        .filter_map(|item| {
            // SAFETY: the name points into `items`, which is alive; the value
            // was asked for as a double.
            unsafe {
                Some((
                    item.szName.to_string().ok()?,
                    item.FmtValue.Anonymous.doubleValue,
                ))
            }
        })
        .collect();
    Some(values)
}
