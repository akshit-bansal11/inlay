//! Spotify through MPRIS (D-Bus), using the `mpris` crate.

use std::time::Duration;

use mpris::{PlaybackStatus, Player, PlayerFinder};

use super::Media;
use crate::model::{Command, Track};

#[derive(Default)]
pub struct Spotify {
    player: Option<Player>,
}

impl Spotify {
    fn read(player: &Player) -> Result<Option<Track>, mpris::DBusError> {
        let status = player.get_playback_status()?;
        if status == PlaybackStatus::Stopped {
            return Ok(None);
        }
        let meta = player.get_metadata()?;
        // ponytail: Spotify's MPRIS Position has been reported to stay at 0 on some
        // versions; the app then extrapolates from the track start, so position-based
        // lyric sync may lag.
        let position = player.get_position()?;
        Ok(Some(Track {
            title: meta.title().unwrap_or_default().to_string(),
            artist: meta.artists().unwrap_or_default().join(", "),
            album: meta.album_name().unwrap_or_default().to_string(),
            duration_ms: meta.length().map_or(0, |d| d.as_millis() as i64),
            position_ms: position.as_millis() as i64,
            playing: status == PlaybackStatus::Playing,
            art_url: meta.art_url().map(str::to_string),
        }))
    }

    fn try_send(player: &Player, command: Command) -> Result<(), mpris::DBusError> {
        match command {
            Command::PlayPause => player.play_pause(),
            Command::Next => player.next(),
            Command::Previous => player.previous(),
            Command::Seek(ms) => {
                // SetPosition needs the current track id, to avoid racing a track change.
                let Some(id) = player.get_metadata()?.track_id() else {
                    return Ok(());
                };
                player.set_position(id, &Duration::from_millis(ms.max(0) as u64))
            }
        }
    }
}

impl Media for Spotify {
    fn poll(&mut self) -> Option<Track> {
        if self.player.is_none() {
            // Identity of Spotify's MPRIS service is "Spotify"; matching ignores case.
            self.player = PlayerFinder::new()
                .ok()
                .and_then(|finder| finder.find_by_name("spotify").ok());
        }
        match Self::read(self.player.as_ref()?) {
            Ok(track) => track,
            Err(_) => {
                // Spotify quit or stopped answering; look it up again next time.
                self.player = None;
                None
            }
        }
    }

    fn send(&mut self, command: Command) {
        let Some(player) = &self.player else { return };
        if let Err(e) = Self::try_send(player, command) {
            eprintln!("could not control Spotify: {e}");
        }
    }
}
