//! The event loop and the widget windows.
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, CentralPanel, ColorImage, Context, Rect, TextureHandle, TextureOptions, Ui, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId, WindowLevel,
};
use inlay_core::color::{Rgba, accent_from_bgra, to_hex};
use inlay_core::config::{Frame, Layer};
use inlay_core::lrclib::Query;

use crate::audio::Spectrum;
use crate::model::{Command, Lyrics, LyricsState, Track};
use crate::settings::Settings;
use crate::tray::{self, Tray, TrayAction};
use crate::{art, clock, fonts, lyrics, lyrics_view, media, paths, player, visualizer};

/// How often config.toml is checked for a hand edit.
const RELOAD_EVERY: Duration = Duration::from_secs(2);
/// How often Spotify is asked what is playing.
const MEDIA_EVERY: Duration = Duration::from_millis(500);
/// Slower polling while nothing plays, so a closed Spotify is not probed (an osascript run) twice a second.
const MEDIA_IDLE: Duration = Duration::from_secs(2);
/// The player's progress bar moves this often while playing.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
/// Spotify green, used when the accent is automatic and there is no album art (as on Windows).
const FALLBACK_ACCENT: Rgba = [0.118, 0.843, 0.376, 1.0];

/// The latest track and when it was read.
type Playing = Arc<Mutex<Option<(Track, Instant)>>>;

/// Starts the event loop and blocks until the app quits.
pub fn run() -> eframe::Result {
    let settings = Settings::load();
    // The root window only hosts the event loop; every widget is its own viewport.
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Inlay")
            .with_inner_size([1.0, 1.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_mouse_passthrough(true)
            .with_window_level(WindowLevel::AlwaysOnBottom),
        ..Default::default()
    };
    eframe::run_native(
        "Inlay",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            fonts::install(&ctx, &settings.cfg, paths::fonts_dir().as_deref());
            let (commands, playing) = spawn_media(ctx.clone());
            let tray = {
                let ctx = ctx.clone();
                Tray::new(move || ctx.request_repaint())
            };
            Ok(Box::new(InlayApp {
                settings,
                reload_at: Instant::now() + RELOAD_EVERY,
                tray,
                commands,
                playing,
                track: None,
                read_at: Instant::now(),
                track_gen: 0,
                lyrics: Arc::default(),
                art_out: Arc::default(),
                art_url: None,
                art: None,
                accent: FALLBACK_ACCENT,
                spectrum: None,
                spectrum_failed: false,
            }))
        }),
    )
}

/// Polls Spotify on its own thread (the Linux source is not `Send`, so it is created
/// there) and runs the commands sent to it. The thread ends when the sender is dropped.
fn spawn_media(ctx: Context) -> (Sender<Command>, Playing) {
    let (commands, inbox) = channel::<Command>();
    let playing: Playing = Arc::default();
    let shared = Arc::clone(&playing);
    std::thread::spawn(move || {
        let mut source = media::open();
        loop {
            let track = source.poll().map(|t| (t, Instant::now()));
            let last_playing = track.as_ref().is_some_and(|(t, _)| t.playing);
            if let Ok(mut slot) = shared.lock() {
                let changed = match (&*slot, &track) {
                    (Some((a, _)), Some((b, _))) => !same_state(a, b),
                    (None, None) => false,
                    _ => true,
                };
                *slot = track;
                if changed {
                    ctx.request_repaint();
                }
            }
            match inbox.recv_timeout(if last_playing {
                MEDIA_EVERY
            } else {
                MEDIA_IDLE
            }) {
                Ok(command) => source.send(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    (commands, playing)
}

/// Whether two reads are the same except for the moving position.
fn same_state(a: &Track, b: &Track) -> bool {
    a.title == b.title
        && a.artist == b.artist
        && a.album == b.album
        && a.duration_ms == b.duration_ms
        && a.playing == b.playing
        && a.art_url == b.art_url
        && (a.playing || a.position_ms == b.position_ms)
}

struct InlayApp {
    settings: Settings,
    reload_at: Instant,
    tray: Option<Tray>,
    commands: Sender<Command>,
    playing: Playing,
    track: Option<Track>,
    read_at: Instant,
    /// Bumped per track, so a slow lyrics lookup cannot fill in the next track.
    track_gen: u64,
    lyrics: Arc<Mutex<LyricsState>>,
    art_out: Arc<Mutex<Option<(String, ColorImage)>>>,
    art_url: Option<String>,
    art: Option<TextureHandle>,
    accent: Rgba,
    spectrum: Option<Spectrum>,
    /// Starting the spectrum failed; not retried until the config changes.
    spectrum_failed: bool,
}

impl eframe::App for InlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    // Runs before `ui` every frame, and also when every widget is covered and `ui` is not
    // called, so the tray, config reload and track sync never stall.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        let reload_in = self.reload_config(ctx);
        self.sync_track(ctx);
        self.take_art(ctx);
        self.sync_spectrum(ctx);
        ctx.request_repaint_after(reload_in);
    }

    // Viewports can only be shown from `ui`; `logic` may not show any ui.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut wake = RELOAD_EVERY;

        // ponytail: 1920x1080 until egui reports the monitor; multi-monitor work areas
        // come with a monitor picker.
        let area = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));
        let auto_cfg;
        let cfg = if self.settings.cfg.theme.accent == "auto" {
            let mut c = self.settings.cfg.clone();
            c.theme.accent = to_hex(self.accent);
            auto_cfg = c;
            &auto_cfg
        } else {
            &self.settings.cfg
        };
        let screen = Screen {
            area,
            scale: cfg.general.layout_scale(area.y),
            layer: cfg.general.layer,
        };
        let scale = screen.scale;
        let position_ms = self.position_ms();
        let with_spotify = self.track.is_some() || !cfg.general.hide_without_spotify;
        let playing = self.track.as_ref().is_some_and(|t| t.playing);

        if cfg.clock.enabled {
            let time = clock::local_now();
            widget(
                &ctx,
                "clock",
                &cfg.clock.frame,
                &screen,
                false,
                |ui, rect| {
                    clock::draw(ui.painter(), rect, cfg, &time, scale);
                },
            );
            wake = wake.min(clock::next_change(&cfg.clock, &time));
        }
        if cfg.player.enabled && with_spotify {
            let mut command = None;
            let track = self.track.as_ref();
            let art = self.art.as_ref();
            widget(
                &ctx,
                "player",
                &cfg.player.frame,
                &screen,
                true,
                |ui, rect| {
                    command = player::draw(ui, rect, cfg, track, position_ms, art, scale);
                },
            );
            if let Some(command) = command {
                if self.commands.send(command).is_err() {
                    eprintln!("inlay: the media thread has stopped");
                }
                ctx.request_repaint();
            }
            if playing {
                wake = wake.min(PROGRESS_EVERY);
            }
        }
        if cfg.lyrics.enabled && with_spotify {
            let mut next = None;
            let state = &self.lyrics;
            widget(
                &ctx,
                "lyrics",
                &cfg.lyrics.frame,
                &screen,
                false,
                |ui, rect| {
                    if let Ok(state) = state.lock() {
                        next = lyrics_view::draw(
                            ui.painter(),
                            rect,
                            cfg,
                            &state.lyrics,
                            position_ms,
                            scale,
                        );
                    }
                },
            );
            if playing && let Some(next) = next {
                wake = wake.min(next);
            }
        }
        if cfg.visualizer.enabled
            && with_spotify
            && let Some(spectrum) = &self.spectrum
        {
            let levels = spectrum.levels();
            let frame = cfg.visualizer.placed(area.x as i32);
            widget(&ctx, "visualizer", &frame, &screen, false, |ui, rect| {
                visualizer::draw(ui.painter(), rect, cfg, &levels, scale);
            });
            if spectrum.active() {
                wake = wake.min(Duration::from_secs_f32(
                    1.0 / cfg.visualizer.fps.max(1) as f32,
                ));
            }
        }
        ctx.request_repaint_after(wake);
    }
}

impl InlayApp {
    fn handle_tray(&self, ctx: &Context) {
        while let Some(action) = self.tray.as_ref().and_then(Tray::poll) {
            match action {
                TrayAction::OpenConfig => tray::open_config(),
                TrayAction::OpenFonts => tray::open_fonts(),
                TrayAction::Quit => {
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                }
            }
        }
    }

    /// Re-reads config.toml when due and applies what a change needs. Returns how long
    /// until the next check.
    fn reload_config(&mut self, ctx: &Context) -> Duration {
        let now = Instant::now();
        if now >= self.reload_at {
            self.reload_at = now + RELOAD_EVERY;
            let before = (
                self.settings.cfg.lyrics.enabled,
                self.settings.cfg.lyrics.word_sync,
            );
            if self.settings.reload_if_changed() {
                fonts::install(ctx, &self.settings.cfg, paths::fonts_dir().as_deref());
                // Dropped here, started again by `sync_spectrum` with the new settings.
                self.spectrum = None;
                self.spectrum_failed = false;
                let cfg = &self.settings.cfg;
                if before != (cfg.lyrics.enabled, cfg.lyrics.word_sync) {
                    self.restart_lyrics(ctx);
                }
            }
        }
        self.reload_at.saturating_duration_since(now)
    }

    /// Takes the media thread's latest read; a different track restarts lyrics and art.
    fn sync_track(&mut self, ctx: &Context) {
        let latest = self.playing.lock().ok().and_then(|slot| slot.clone());
        let (track, read_at) = match latest {
            Some((track, at)) => (Some(track), at),
            None => (None, self.read_at),
        };
        let is_new = match (&self.track, &track) {
            (Some(a), Some(b)) => {
                (&a.title, &a.artist, &a.album) != (&b.title, &b.artist, &b.album)
            }
            (None, None) => false,
            _ => true,
        };
        self.track = track;
        self.read_at = read_at;
        if !is_new {
            return;
        }
        self.restart_lyrics(ctx);
        self.art = None;
        self.accent = FALLBACK_ACCENT;
        self.art_url = self.track.as_ref().and_then(|t| t.art_url.clone());
        if let Ok(mut slot) = self.art_out.lock() {
            *slot = None;
        }
        if let Some(url) = &self.art_url {
            let ctx = ctx.clone();
            art::fetch(
                url.clone(),
                Arc::clone(&self.art_out),
                Arc::new(move || ctx.request_repaint()),
            );
        }
    }

    /// Looks the current track's lyrics up again, dropping any lookup still running.
    fn restart_lyrics(&mut self, ctx: &Context) {
        self.track_gen += 1;
        let cfg = &self.settings.cfg;
        let lyrics = match &self.track {
            Some(_) => Lyrics::Loading,
            None => Lyrics::None,
        };
        if let Ok(mut state) = self.lyrics.lock() {
            *state = LyricsState {
                track_gen: self.track_gen,
                lyrics,
            };
        }
        let Some(track) = self.track.as_ref().filter(|_| cfg.lyrics.enabled) else {
            return;
        };
        let ctx = ctx.clone();
        lyrics::fetch(
            Query {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_ms: track.duration_ms,
            },
            self.track_gen,
            Arc::clone(&self.lyrics),
            cfg.lyrics
                .word_sync
                .then(|| cfg.lyrics.word_servers.clone()),
            Arc::new(move || ctx.request_repaint()),
        );
    }

    /// Turns a finished art download into a texture and the accent colour.
    fn take_art(&mut self, ctx: &Context) {
        let Some((url, image)) = self.art_out.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        // Art of a track already skipped.
        if self.art_url.as_deref() != Some(url.as_str()) {
            return;
        }
        let bgra: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(|p| [p.b(), p.g(), p.r(), p.a()])
            .collect();
        self.accent = accent_from_bgra(&bgra, FALLBACK_ACCENT);
        self.art = Some(ctx.load_texture("art", image, TextureOptions::LINEAR));
    }

    /// Runs audio capture only while the visualizer is on and Spotify is playing.
    fn sync_spectrum(&mut self, ctx: &Context) {
        // Capture only while Spotify plays: the default input may be the microphone, and listening while idle would keep the app awake and the mic in use.
        let playing = self.track.as_ref().is_some_and(|t| t.playing);
        if !(self.settings.cfg.visualizer.enabled && playing) {
            self.spectrum = None;
            self.spectrum_failed = false;
        } else if self.spectrum.is_none() && !self.spectrum_failed {
            let ctx = ctx.clone();
            self.spectrum = Spectrum::start(
                &self.settings.cfg.visualizer,
                Arc::new(move || ctx.request_repaint()),
            );
            self.spectrum_failed = self.spectrum.is_none();
        }
    }

    /// Where playback is now: the last read, moved on by the time since while playing.
    fn position_ms(&self) -> i64 {
        let Some(track) = &self.track else {
            return 0;
        };
        let moved = if track.playing {
            i64::try_from(self.read_at.elapsed().as_millis()).unwrap_or(0)
        } else {
            0
        };
        (track.position_ms + moved).clamp(0, track.duration_ms.max(0))
    }
}

/// What every widget window needs to know about the screen.
struct Screen {
    /// The work area in display-independent pixels.
    area: Vec2,
    scale: f32,
    layer: Layer,
}

/// Shows one widget as its own transparent, undecorated window at its frame and runs
/// `draw` on its full rect. Only an `interactive` widget takes mouse clicks.
fn widget(
    ctx: &Context,
    name: &str,
    frame: &Frame,
    screen: &Screen,
    interactive: bool,
    mut draw: impl FnMut(&mut Ui, Rect),
) {
    let (x, y) = frame.origin_within(screen.area.x as i32, screen.area.y as i32);
    let level = match screen.layer {
        Layer::Desktop => WindowLevel::AlwaysOnBottom,
        Layer::Normal => WindowLevel::Normal,
        Layer::Top => WindowLevel::AlwaysOnTop,
    };
    let builder = ViewportBuilder::default()
        .with_title(format!("Inlay {name}"))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_mouse_passthrough(!interactive)
        .with_window_level(level)
        .with_inner_size([frame.w as f32 * screen.scale, frame.h as f32 * screen.scale])
        // On Wayland a client cannot place its own windows, so the compositor ignores this.
        .with_position([x as f32, y as f32]);
    ctx.show_viewport_immediate(ViewportId::from_hash_of(name), builder, |ui, _class| {
        CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            draw(ui, rect);
        });
    });
}
