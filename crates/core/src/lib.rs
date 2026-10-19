//! Everything in Inlay that does not touch Windows: pure functions over
//! text, time and numbers, so all of it runs under `cargo test`.

#![forbid(unsafe_code)]

pub mod bands;
pub mod clock;
pub mod color;
pub mod config;
pub mod lrc;
pub mod lrclib;
pub mod lyricsplus;
pub mod mark;
pub mod timefmt;
pub mod timing;
