//! Reading Spotify and controlling it: AppleScript on macOS, MPRIS (D-Bus) on Linux.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use crate::model::{Command, Track};

/// A source of the current track and a way to control it.
pub trait Media {
    /// The current track, or `None` when Spotify is not running or nothing is loaded.
    /// Blocks briefly (a D-Bus call or an osascript run), so call it at most a few times a second.
    fn poll(&mut self) -> Option<Track>;
    /// Sends a command; failures are logged and otherwise ignored.
    fn send(&mut self, command: Command);
}

/// The media source for this system.
pub fn open() -> Box<dyn Media> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::Spotify);
    #[cfg(target_os = "linux")]
    return Box::new(linux::Spotify::default());
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Box::new(NoMedia);
}

/// Other unix systems have no supported way to reach Spotify.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
struct NoMedia;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Media for NoMedia {
    fn poll(&mut self) -> Option<Track> {
        None
    }
    fn send(&mut self, _command: Command) {}
}
