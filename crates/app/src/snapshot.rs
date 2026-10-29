//! Renders every widget into PNG files from made-up data: no window, GPU,
//! Spotify or audio device needed. CI uploads the result, so a change to the
//! design can be looked at without running the app.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use inlay_core::color::{accent_from_bgra, downsample_bgra};
use inlay_core::config::{
    Config, Direction, Letters, LyricsMode, PlayerLayout, Preset, VisualizerStyle,
};
use inlay_core::lrc::parse_lrc;
use inlay_core::timefmt::LocalTime;
use inlay_core::timing::normalize_lines;
use windows::Win32::Foundation::{E_FAIL, GENERIC_WRITE};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{Error, HSTRING, Result};

use crate::app::Palette;
use crate::capture::Audio;
use crate::gfx::{Gfx, rect};
use crate::lyrics::Lyrics;
use crate::media::{Art, BLUR_SIDE, MediaState};
use crate::menu;
use crate::settings;
use crate::widgets::{Ctx, Kind};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 900;
const ART_SIDE: u32 = 160;
/// Where the pretend track is, chosen to land on a lyric line.
const POSITION_MS: f64 = 61_500.0;
/// Ticks run before drawing, so bars have risen and scrolling has settled.
const WARM_UP_TICKS: u32 = 40;

const LYRICS: &str = "\
[00:44.00] We left the porch light burning
[00:49.50] for anyone still on the road
[00:55.00] The radio kept turning
[01:00.00] every mile into a song we know
[01:06.00] So roll the windows down
[01:11.00] and let the night come in
[01:16.00] We are not lost, we are not found
[01:22.00] we are somewhere in between";

/// A teal-to-magenta gradient with a pale disc: enough colour for the
/// accent and the blurred background to have something to work with.
fn fake_art() -> Art {
    let side = ART_SIDE as usize;
    let mut bgra = Vec::with_capacity(side * side * 4);
    for i in 0..side * side {
        let (x, y) = (
            (i % side) as f32 / side as f32,
            (i / side) as f32 / side as f32,
        );
        let t = (x + y) / 2.0;
        let disc = if (x - 0.68).hypot(y - 0.34) < 0.2 {
            0.8
        } else {
            0.0
        };
        let channel = |from: f32, to: f32| {
            let base = from + (to - from) * t;
            (base + (235.0 - base) * disc) as u8
        };
        bgra.extend([
            channel(170.0, 120.0),
            channel(150.0, 40.0),
            channel(20.0, 210.0),
            255,
        ]);
    }
    Art {
        accent: Some(accent_from_bgra(&bgra, [1.0; 4])),
        blur: downsample_bgra(&bgra, ART_SIDE, ART_SIDE, BLUR_SIDE),
        w: ART_SIDE,
        h: ART_SIDE,
        bgra,
    }
}

/// Many partials from 45 Hz to 14 kHz, louder in the bass and with a few
/// humps, so the bars have the broad falling shape of music rather than the
/// isolated spikes of a test tone.
fn fake_audio() -> Audio {
    let audio = Audio::offline();
    let partials: Vec<(f32, f32, f32)> = (0..140)
        .map(|k| {
            let k = k as f32;
            let hump = 0.5 + 0.5 * (k * 0.21).sin();
            (
                45.0 * 1.042f32.powf(k),
                0.05 / (1.0 + k / 14.0) * (0.25 + 0.75 * hump * hump),
                k * k * 0.37,
            )
        })
        .collect();
    let samples: Vec<f32> = (0..4096)
        .map(|n| {
            let t = n as f32 / 48_000.0;
            partials
                .iter()
                .map(|(hz, amplitude, phase)| {
                    amplitude * (std::f32::consts::TAU * hz * t + phase).sin()
                })
                .sum()
        })
        .collect();
    audio.push(&samples, 48_000);
    audio.active.store(true, Ordering::SeqCst);
    audio
}

fn fake_media() -> MediaState {
    MediaState {
        present: true,
        playing: true,
        title: "Somewhere in Between".to_owned(),
        artist: "The Night Drivers".to_owned(),
        album: "Porch Lights".to_owned(),
        duration_ms: 214_000.0,
        position_ms: POSITION_MS,
        position_at_ms: 0.0,
        art: Some(Arc::new(fake_art())),
        track_gen: 1,
        art_gen: 1,
        update_gen: 1,
    }
}

pub fn run(dir: &Path) -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    std::fs::create_dir_all(dir).map_err(|_| Error::from(E_FAIL))?;
    let wic: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };

    let media = fake_media();
    let audio = fake_audio();
    let lyrics = Lyrics::Synced(normalize_lines(
        &parse_lrc(LYRICS).lines,
        media.duration_ms as i64,
    ));

    let mut looks = vec![("default".to_owned(), Config::default())];
    for preset in Preset::ALL {
        let mut cfg = Config::default();
        cfg.apply_preset(preset);
        looks.push((preset.name().to_lowercase(), cfg));
    }
    // One more to cover the options no preset turns on.
    let mut variant = Config::default();
    variant.lyrics.mode = LyricsMode::Progress;
    variant.visualizer.style = VisualizerStyle::Wave;
    variant.visualizer.symmetric = true;
    variant.visualizer.card = true;
    variant.visualizer.flip_x = true;
    // The date moved above the time and the day beside it.
    variant.clock.date.y = 8.0;
    (variant.clock.day.x, variant.clock.day.y) = (250.0, 150.0);
    variant.clock.date.size = 28.0;
    variant.player.bar_height = 10.0;
    variant.player.title.font = "Georgia".to_owned();
    variant.player.elapsed.size = 16.0;
    variant.lyrics.inactive_color = "accent".to_owned();
    variant.lyrics.word_color = "#F9F871".to_owned();
    variant.lyrics.stroke_width = 2.0;
    variant.player.arrange(PlayerLayout::Centered);
    variant.clock.time_format = "%l:%M %p".to_owned();
    looks.push(("word-wave".to_owned(), variant));
    // Bars hanging from the top edge of the screen.
    let mut hanging = Config::default();
    hanging.visualizer.flip_y = true;
    hanging.visualizer.frame.anchor = inlay_core::config::Anchor::Top;
    hanging.clock.frame.y = 200;
    // Text every way it can run: a column of upright letters, a line on its
    // side reading down, and one reading up.
    hanging.clock.day.direction = Direction::Down;
    hanging.clock.date.direction = Direction::Down;
    hanging.clock.date.letters = Letters::Sideways;
    (hanging.clock.date.x, hanging.clock.date.y) = (120.0, 0.0);
    hanging.player.artist.direction = Direction::Up;
    hanging.player.artist.letters = Letters::Sideways;
    looks.push(("flipped".to_owned(), hanging));

    for (name, cfg) in &looks {
        let path = dir.join(format!("{name}.png"));
        png(&wic, &path, (WIDTH, HEIGHT), |gfx| {
            scene(gfx, cfg, &media, &lyrics, &audio)
        })?;
        println!("wrote {}", path.display());
    }

    // One page of each tab, and one of an element's own.
    let panel = (settings::WIDTH as u32, settings::HEIGHT as u32);
    for (name, tab, section) in [
        ("general", 0, 0),
        ("clock", 1, 0),
        ("clock-day", 1, 1),
        ("player-buttons", 2, 4),
        ("lyrics", 3, 0),
        ("visualizer", 4, 0),
    ] {
        let path = dir.join(format!("settings-{name}.png"));
        png(&wic, &path, panel, |gfx| settings_page(gfx, tab, section))?;
        println!("wrote {}", path.display());
    }

    // The tray menu, with the pointer on its second row.
    let card = (menu::WIDTH as u32, menu::HEIGHT as u32);
    let path = dir.join("tray-menu.png");
    png(&wic, &path, card, |gfx| {
        menu::paint(gfx, (40.0, 120.0), true)
    })?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Draws into a new bitmap of `size` and saves it as a PNG.
fn png(
    wic: &IWICImagingFactory,
    path: &Path,
    size: (u32, u32),
    draw: impl FnOnce(&mut Gfx) -> Result<()>,
) -> Result<()> {
    let bitmap = unsafe {
        wic.CreateBitmap(
            size.0,
            size.1,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnLoad,
        )?
    };
    let mut gfx = Gfx::for_bitmap(&bitmap)?;
    gfx.begin_draw();
    let drawn = draw(&mut gfx);
    gfx.end_draw()?;
    drawn?;
    drop(gfx);

    unsafe {
        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(path.as_os_str()), GENERIC_WRITE.0)?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;

        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(|| Error::from(E_FAIL))?;
        frame.Initialize(None)?;
        frame.SetSize(size.0, size.1)?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut format)?;
        frame.WriteSource(&bitmap, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()
    }
}

/// All four widgets where the config puts them, over a stand-in wallpaper.
fn scene(
    gfx: &mut Gfx,
    cfg: &Config,
    media: &MediaState,
    lyrics: &Lyrics,
    audio: &Audio,
) -> Result<()> {
    let palette = Palette::new(cfg);
    let ctx_at = |now_ms: f64| Ctx {
        cfg,
        media,
        lyrics,
        lyrics_gen: 1,
        audio,
        now_ms,
        time: LocalTime {
            year: 2026,
            month: 10,
            day: 3,
            weekday: 6,
            hour: 21,
            minute: 47,
            second: 12,
        },
        ms_to_next_second: 500,
        text: palette.text,
        card: palette.card,
        accent: palette.accent(media),
    };

    // Widgets are translucent, so they need a wallpaper behind them to judge.
    let wallpaper = gfx.vertical_gradient(
        0.0,
        HEIGHT as f32,
        [0.09, 0.11, 0.24, 1.0],
        [0.44, 0.21, 0.36, 1.0],
    )?;
    gfx.fill_round_with(rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32), 0.0, &wallpaper);

    for kind in Kind::ALL {
        if !kind.enabled(cfg) {
            continue;
        }
        let frame = kind.placed(cfg, WIDTH as i32);
        let (x, y) = frame.origin(WIDTH as i32, HEIGHT as i32);
        let (w, h) = (frame.w as f32, frame.h as f32);
        let mut widget = kind.create();
        for step in 0..WARM_UP_TICKS {
            widget.tick(&ctx_at(f64::from(step) * 16.7));
        }
        let ctx = ctx_at(f64::from(WARM_UP_TICKS) * 16.7);
        gfx.set_transform(1.0, x as f32, y as f32);
        widget.draw(gfx, w, h, &ctx)?;
    }
    Ok(())
}

/// One section of one tab of the settings panel. The first has the colour
/// picker open: nothing else can click it open here.
fn settings_page(gfx: &mut Gfx, tab: usize, section: usize) -> Result<()> {
    let mut cfg = Config::default();
    let mut view = if (tab, section) == (0, 0) {
        settings::View::with_picker_open()
    } else {
        settings::View::on(tab, section)
    };
    view.paint(
        gfx,
        &mut cfg,
        (settings::WIDTH, settings::HEIGHT),
        menu::ORANGE,
        settings::Status {
            edit: false,
            hidden: false,
            autostart: true,
            scale: 1.25,
        },
        (WIDTH as i32, HEIGHT as i32),
    )
    .map(|_| ())
}
