//! The types the parts of the macOS and Linux front-end hand each other: what is
//! playing, what to tell the player, and the lyrics found for it.

use inlay_core::timing::Line;

/// What Spotify reports about the current track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    /// Where playback was when this was read; it moves on while `playing`.
    pub position_ms: i64,
    pub playing: bool,
    /// Album art, as an https URL Spotify serves.
    pub art_url: Option<String>,
}

/// What a click on the player asks Spotify to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    PlayPause,
    Next,
    Previous,
    /// Jump to this many milliseconds into the track.
    Seek(i64),
}

/// The lyrics for the current track, as far as they are known.
#[derive(Clone, Debug, Default)]
pub enum Lyrics {
    /// Nothing is playing.
    #[default]
    None,
    Loading,
    Synced(Vec<Line>),
    /// Text without timestamps.
    Plain(Vec<String>),
    Instrumental,
    /// The lyric services know no lyrics for this track.
    Missing,
    /// The lyric services could not be reached.
    Unavailable,
}

/// Lyrics with the track they belong to. A lookup that finishes after the track
/// changed must not overwrite the newer track's lyrics, so each lookup carries
/// the generation it was started for.
#[derive(Debug, Default)]
pub struct LyricsState {
    pub track_gen: u64,
    pub lyrics: Lyrics,
}
