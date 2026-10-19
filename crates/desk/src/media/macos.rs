//! Spotify through its AppleScript dictionary. The first poll makes macOS ask once
//! whether Inlay may control Spotify (Automation permission); declining leaves the
//! player empty.

use std::process::Command as Process;

use super::Media;
use crate::model::{Command, Track};

const OSASCRIPT: &str = "/usr/bin/osascript";

// `is running` keeps this from launching Spotify.
const POLL: &str = r#"if application "Spotify" is running then
  tell application "Spotify"
    if player state is stopped then return ""
    set t to current track
    return (name of t) & tab & (artist of t) & tab & (album of t) & tab & (duration of t) & tab & (player position) & tab & (player state as string) & tab & (artwork url of t)
  end tell
end if
return """#;

#[derive(Default)]
pub struct Spotify;

impl Media for Spotify {
    fn poll(&mut self) -> Option<Track> {
        let out = Process::new(OSASCRIPT).arg("-e").arg(POLL).output().ok()?;
        if !out.status.success() {
            return None;
        }
        parse(String::from_utf8_lossy(&out.stdout).trim_end_matches(['\r', '\n']))
    }

    fn send(&mut self, command: Command) {
        let action = match command {
            Command::PlayPause => "playpause".to_string(),
            Command::Next => "next track".to_string(),
            Command::Previous => "previous track".to_string(),
            Command::Seek(ms) => format!("set player position to {:.3}", ms as f64 / 1000.0),
        };
        let script = format!("tell application \"Spotify\" to {action}");
        match Process::new(OSASCRIPT).arg("-e").arg(&script).output() {
            Ok(out) if out.status.success() => {}
            Ok(out) => eprintln!(
                "osascript failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => eprintln!("could not run osascript: {e}"),
        }
    }
}

/// Parses one tab-separated line of the poll script.
fn parse(line: &str) -> Option<Track> {
    let f: Vec<&str> = line.split('\t').collect();
    let [title, artist, album, duration, position, state, art] = f[..] else {
        return None;
    };
    // Some locales print the decimal separator as a comma.
    let secs: f64 = position.replace(',', ".").trim().parse().ok()?;
    Some(Track {
        title: title.to_string(),
        artist: artist.to_string(),
        album: album.to_string(),
        duration_ms: duration.trim().parse().ok()?,
        position_ms: (secs * 1000.0).round() as i64,
        playing: state == "playing",
        art_url: (!art.is_empty()).then(|| art.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_poll_line() {
        let t = parse("Song\tArtist\tAlbum\t215000\t12.5\tplaying\thttps://i.scdn.co/a").unwrap();
        assert_eq!(t.title, "Song");
        assert_eq!(t.duration_ms, 215000);
        assert_eq!(t.position_ms, 12500);
        assert!(t.playing);
        assert_eq!(t.art_url.as_deref(), Some("https://i.scdn.co/a"));

        let t = parse("S\tA\tB\t1000\t3,25\tpaused\t").unwrap();
        assert_eq!(t.position_ms, 3250);
        assert!(!t.playing);
        assert_eq!(t.art_url, None);

        assert!(parse("").is_none());
        assert!(parse("a\tb").is_none());
    }
}
