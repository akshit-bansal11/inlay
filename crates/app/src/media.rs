//! Spotify's track, playback state and controls, through the Windows media
//! session API. No Spotify login, no client ID, no polling: Windows raises an
//! event when anything changes.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use inlay_core::color::{Rgba, accent_from_bgra, downsample_bgra};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::Result;

use crate::capture::target_exe;
use crate::window::WM_APP_MEDIA;
use crate::{Notify, log, now_ms};

/// Side of the tiny image the blurred player background is stretched from.
pub const BLUR_SIDE: u32 = 16;
/// Album art larger than this is not worth the memory for a 100px thumbnail.
const MAX_ART_BYTES: u32 = 8 * 1024 * 1024;

pub struct Art {
    pub w: u32,
    pub h: u32,
    /// Premultiplied BGRA.
    pub bgra: Vec<u8>,
    /// `BLUR_SIDE` squared pixels: the art reduced to a smear of colour.
    pub blur: Vec<u8>,
    pub accent: Option<Rgba>,
}

#[derive(Clone, Default)]
pub struct MediaState {
    /// Spotify has a media session, playing or not.
    pub present: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: f64,
    /// Position reported by Spotify, true at `position_at_ms` on the app clock.
    pub position_ms: f64,
    pub position_at_ms: f64,
    pub art: Option<Arc<Art>>,
    /// Bumped when the title or artist changes.
    pub track_gen: u64,
    /// Bumped when the art changes.
    pub art_gen: u64,
    /// Bumped on every update.
    pub update_gen: u64,
}

impl MediaState {
    /// Where playback is right now, extrapolated from the last report.
    pub fn position_now(&self, now_ms: f64) -> f64 {
        let elapsed = if self.playing {
            now_ms - self.position_at_ms
        } else {
            0.0
        };
        let end = if self.duration_ms > 0.0 {
            self.duration_ms
        } else {
            f64::MAX
        };
        (self.position_ms + elapsed).clamp(0.0, end)
    }
}

pub enum Cmd {
    /// Something changed. `props` is true when it may include new art.
    Refresh {
        props: bool,
    },
    PlayPause,
    Next,
    Previous,
    SeekMs(f64),
}

pub fn spawn(shared: Arc<Mutex<MediaState>>, notify: Notify) -> Sender<Cmd> {
    let (tx, rx) = channel();
    let events = tx.clone();
    std::thread::spawn(move || {
        if let Err(error) = run(&rx, &events, &shared, notify) {
            log(&format!("media session thread stopped: {error}"));
        }
    });
    tx
}

/// 100ns ticks since 1601, the unit WinRT timestamps use.
fn filetime_now() -> i64 {
    const EPOCH_GAP_SECS: i64 = 11_644_473_600;
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (unix.as_secs() as i64 + EPOCH_GAP_SECS) * 10_000_000 + i64::from(unix.subsec_nanos() / 100)
}

/// Spotify's session. The desktop build identifies itself as `Spotify.exe`
/// and the Store build as `SpotifyAB.SpotifyMusic_...!Spotify`; both contain
/// the name.
fn find_spotify(manager: &Manager) -> Option<Session> {
    let name = target_exe()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_owned();
    manager.GetSessions().ok()?.into_iter().find(|session| {
        session
            .SourceAppUserModelId()
            .is_ok_and(|id| id.to_string().to_ascii_lowercase().contains(&name))
    })
}

/// The id of every app with a media session right now, for the probe.
pub fn session_ids() -> Vec<String> {
    let ids = || -> Result<Vec<String>> {
        Manager::RequestAsync()?
            .join()?
            .GetSessions()?
            .into_iter()
            .map(|session| Ok(session.SourceAppUserModelId()?.to_string()))
            .collect()
    };
    ids().unwrap_or_default()
}

fn watch(session: &Session, events: &Sender<Cmd>) -> Result<()> {
    let tx = events.clone();
    session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx.send(Cmd::Refresh { props: true });
        Ok(())
    }))?;
    let tx = events.clone();
    session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx.send(Cmd::Refresh { props: false });
        Ok(())
    }))?;
    let tx = events.clone();
    session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx.send(Cmd::Refresh { props: false });
        Ok(())
    }))?;
    Ok(())
}

fn run(
    rx: &Receiver<Cmd>,
    events: &Sender<Cmd>,
    shared: &Mutex<MediaState>,
    notify: Notify,
) -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let manager = Manager::RequestAsync()?.join()?;

    let tx = events.clone();
    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx.send(Cmd::Refresh { props: true });
        Ok(())
    }))?;
    let _ = events.send(Cmd::Refresh { props: true });

    let mut session: Option<Session> = None;
    let mut state = MediaState::default();

    for cmd in rx {
        let props = match cmd {
            Cmd::Refresh { props } => props,
            // The outcome arrives as a Refresh from the session's own events,
            // so the results of these calls are deliberately not awaited.
            Cmd::PlayPause => {
                let _ = session.as_ref().map(Session::TryTogglePlayPauseAsync);
                continue;
            }
            Cmd::Next => {
                let _ = session.as_ref().map(Session::TrySkipNextAsync);
                continue;
            }
            Cmd::Previous => {
                let _ = session.as_ref().map(Session::TrySkipPreviousAsync);
                continue;
            }
            Cmd::SeekMs(ms) => {
                let ticks = (ms * 10_000.0) as i64;
                let _ = session
                    .as_ref()
                    .map(|s| s.TryChangePlaybackPositionAsync(ticks));
                continue;
            }
        };

        let found = find_spotify(&manager);
        if found != session {
            if let Some(new) = &found
                && let Err(error) = watch(new, events)
            {
                log(&format!("could not watch the Spotify session: {error}"));
            }
            session = found;
        }

        state = match &session {
            Some(current) => read(current, &state, props).unwrap_or_else(|error| {
                // Spotify closing mid-read lands here; the next event corrects it.
                log(&format!("reading the Spotify session failed: {error}"));
                MediaState {
                    update_gen: state.update_gen + 1,
                    track_gen: state.track_gen,
                    art_gen: state.art_gen,
                    ..MediaState::default()
                }
            }),
            None => MediaState {
                update_gen: state.update_gen + 1,
                track_gen: state.track_gen,
                art_gen: state.art_gen,
                ..MediaState::default()
            },
        };
        if let Ok(mut guard) = shared.lock() {
            *guard = state.clone();
        }
        notify.post(WM_APP_MEDIA);
    }
    Ok(())
}

fn read(session: &Session, previous: &MediaState, props_changed: bool) -> Result<MediaState> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let timeline = session.GetTimelineProperties()?;
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing;

    let mut next = MediaState {
        present: true,
        playing,
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        album: props.AlbumTitle()?.to_string(),
        duration_ms: (timeline.EndTime()?.Duration - timeline.StartTime()?.Duration) as f64
            / 10_000.0,
        position_ms: timeline.Position()?.Duration as f64 / 10_000.0,
        art: previous.art.clone(),
        track_gen: previous.track_gen,
        art_gen: previous.art_gen,
        update_gen: previous.update_gen + 1,
        position_at_ms: 0.0,
    };

    // The position was true when Spotify last reported it, which can be a
    // while ago: it reports on play, pause and seek, not continuously.
    let age_ms = (filetime_now() - timeline.LastUpdatedTime()?.UniversalTime) as f64 / 10_000.0;
    let plausible = playing && (0.0..=next.duration_ms.max(1.0)).contains(&age_ms);
    next.position_at_ms = now_ms() - if plausible { age_ms } else { 0.0 };

    let track_changed = next.title != previous.title || next.artist != previous.artist;
    if track_changed {
        next.track_gen += 1;
    }
    if track_changed || props_changed || next.art.is_none() {
        // Spotify publishes the title first and the art a moment later, so a
        // missing thumbnail keeps the old art until the follow-up event.
        match props.Thumbnail().and_then(|thumb| load_art(&thumb)) {
            Ok(art) => {
                let same = previous
                    .art
                    .as_ref()
                    .is_some_and(|old| old.bgra == art.bgra);
                if !same {
                    next.art = Some(Arc::new(art));
                    next.art_gen += 1;
                }
            }
            Err(_) if track_changed => {
                next.art = None;
                next.art_gen += 1;
            }
            Err(_) => {}
        }
    }
    Ok(next)
}

fn load_art(thumbnail: &IRandomAccessStreamReference) -> Result<Art> {
    let stream = thumbnail.OpenReadAsync()?.join()?;
    let size = stream.Size()?.min(u64::from(MAX_ART_BYTES)) as u32;
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.join()?;
    let mut encoded = vec![0u8; size as usize];
    reader.ReadBytes(&mut encoded)?;

    let (w, h, bgra) = decode_image(&encoded)?;
    Ok(Art {
        accent: Some(accent_from_bgra(&bgra, [0.0; 4])).filter(|accent| accent[3] > 0.0),
        blur: downsample_bgra(&bgra, w, h, BLUR_SIDE),
        w,
        h,
        bgra,
    })
}

/// Decodes any image WIC understands into premultiplied BGRA.
pub fn decode_image(encoded: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(encoded)?;
        let frame = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?
            .GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let (mut w, mut h) = (0, 0);
        converter.GetSize(&mut w, &mut h)?;
        // Album art is drawn small; a huge or bogus size is refused rather than allocated.
        const MAX_SIDE: u32 = 4096;
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(windows::core::Error::from(
                windows::Win32::Foundation::E_FAIL,
            ));
        }
        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        converter.CopyPixels(std::ptr::null(), w * 4, &mut pixels)?;
        Ok((w, h, pixels))
    }
}
