//! Inlay: live widgets set into the Windows desktop.
//!
//! `inlay` runs the app. `inlay --snapshot <dir>` renders the
//! widgets to PNG files from made-up data.
//!
//! Three diagnostics for a machine where a widget stays empty. There is no
//! console, so redirect their output to a file to read it:
//! - `--probe` prints what Windows reports about Spotify, and the level of
//!   Spotify's audio, for eight seconds. `--probe <name.exe>` listens to that
//!   app instead, to test per-app capture without Spotify running.
//! - `--probe-system` does the same but listens to everything the PC plays.
//! - `--probe-lyrics <title> <artist>` looks one track up, for word timing
//!   and on LRCLIB.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod capture;
mod gfx;
mod lyrics;
mod media;
mod menu;
mod settings;
mod snapshot;
mod tray;
mod widgets;
mod window;

use std::ffi::c_void;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use inlay_core::config::AudioSource;
use windows::Win32::Foundation::HWND;
use windows::core::Result;

const APP_DIR: &str = "inlay";
/// The folder names under the app's earlier names, newest first: Deskbeat,
/// then Sonic Veil.
const OLD_APP_DIRS: [&str; 2] = ["deskbeat", "sonic-veil"];
const LOG_LIMIT_BYTES: u64 = 512 * 1024;

/// Milliseconds since the app started, on a clock that never goes backwards.
/// Every timestamp shared between threads is on this clock.
pub fn now_ms() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

fn env_dir(variable: &str) -> PathBuf {
    std::env::var_os(variable)
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(APP_DIR)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Brings the settings, fonts and cache kept under an earlier name over to
/// this one, once: a rename must not cost anyone their setup. Only the
/// newest earlier folder is taken. A folder that cannot be moved, because
/// the old app is still running, is copied.
fn adopt_old_folders() {
    let mut failures = Vec::new();
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        let new = env_dir(variable);
        if new.exists() {
            continue;
        }
        let Some(old) = OLD_APP_DIRS
            .iter()
            .map(|name| new.with_file_name(name))
            .find(|old| old.is_dir())
        else {
            continue;
        };
        if std::fs::rename(&old, &new).is_err()
            && let Err(error) = copy_dir(&old, &new)
        {
            failures.push(format!("could not bring {} over: {error}", old.display()));
        }
    }
    // Logged afterwards: logging creates one of the folders being moved.
    for failure in failures {
        log(&failure);
    }
}

/// Where the config file lives: it roams with the user.
pub fn config_dir() -> PathBuf {
    env_dir("APPDATA")
}

/// Where the log and the lyrics cache live: machine-local, safe to delete.
pub fn data_dir() -> PathBuf {
    env_dir("LOCALAPPDATA")
}

/// Appends a line to the log file. There is no console and no telemetry, so
/// this file is the only place a failure can be seen.
pub fn log(message: &str) {
    let dir = data_dir();
    let path = dir.join("inlay.log");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > LOG_LIMIT_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    let (t, _) = app::local_time();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{}-{:02}-{:02} {:02}:{:02}:{:02} {message}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
    }
}

/// Lets a worker thread wake the UI thread by posting to its main window.
/// A window handle is just a number, which is what makes this safe to send.
#[derive(Clone, Copy)]
pub struct Notify(isize);

impl Notify {
    pub fn new(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    /// For runs with no window to notify.
    pub fn none() -> Self {
        Self(0)
    }

    pub fn post(self, msg: u32) {
        if self.0 != 0 {
            window::post(HWND(self.0 as *mut c_void), msg);
        }
    }
}

/// Looks one track up the way the lyrics widget would, and prints what came back.
fn probe_lyrics(title: &str, artist: &str) {
    let query = inlay_core::lrclib::Query {
        title: title.to_owned(),
        artist: artist.to_owned(),
        album: String::new(),
        duration_ms: 0,
    };
    let servers = inlay_core::lyricsplus::SERVERS.map(str::to_owned);
    match lyrics::words(&query, &servers) {
        Some(lines) => {
            println!("word-timed, {} lines", lines.len());
            for word in lines.iter().take(2).flat_map(|line| &line.words) {
                println!("  {:>6}..{:<6} {}", word.start_ms, word.end_ms, word.text);
            }
        }
        None => println!("no word timing found"),
    }
    match lyrics::lookup(&query) {
        lyrics::Lyrics::Synced(lines) => {
            println!("synced, {} lines", lines.len());
            for line in lines.iter().take(4) {
                println!("  {:>6}ms  {}", line.start_ms, line.text);
            }
        }
        lyrics::Lyrics::Plain(lines) => println!("plain, {} lines", lines.len()),
        lyrics::Lyrics::Instrumental => println!("instrumental"),
        _ => println!("no lyrics found"),
    }
}

/// Prints what the media session and the audio capture report, twice a second.
fn probe(source: AudioSource) -> Result<()> {
    let shared = Arc::new(Mutex::new(media::MediaState::default()));
    let _controls = media::spawn(shared.clone(), Notify::none());
    let audio = capture::spawn(Notify::none())?;
    audio.set_source(Some(source));
    println!(
        "following {}: root process {:?}",
        capture::target_exe(),
        capture::spotify_pid()
    );
    std::thread::sleep(Duration::from_millis(300));
    println!("apps with a media session: {:?}", media::session_ids());

    let mut samples = vec![0.0f32; 2048];
    for _ in 0..16 {
        std::thread::sleep(Duration::from_millis(500));
        audio.kick();
        let state = shared.lock().map(|s| s.clone()).unwrap_or_default();
        let rate = audio.latest(&mut samples);
        let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        println!(
            "present={} playing={} title={:?} artist={:?} album={:?} duration={:.0}ms \
             position={:.0}ms (reported {:.0}ms, {:.0}ms ago) art={:?} accent={:?} | \
             audio rate={rate} peak={peak:.4}",
            state.present,
            state.playing,
            state.title,
            state.artist,
            state.album,
            state.duration_ms,
            state.position_now(now_ms()),
            state.position_ms,
            now_ms() - state.position_at_ms,
            state.art.as_ref().map(|art| (art.w, art.h)),
            state.art.as_ref().and_then(|art| art.accent),
        );
    }
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    now_ms();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--snapshot") => {
            snapshot::run(Path::new(args.get(1).map_or("snapshots", String::as_str)))
        }
        Some("--probe") => {
            if let Some(exe) = args.get(1) {
                capture::set_target_exe(exe.clone());
            }
            probe(AudioSource::Spotify)
        }
        Some("--probe-system") => probe(AudioSource::System),
        Some("--probe-lyrics") => {
            let arg = |i: usize| args.get(i).map_or("", String::as_str);
            probe_lyrics(arg(1), arg(2));
            Ok(())
        }
        _ => {
            adopt_old_folders();
            app::run()
        }
    };
    if let Err(error) = result {
        log(&format!("fatal: {error}"));
        eprintln!("{error}");
        std::process::exit(1);
    }
}
