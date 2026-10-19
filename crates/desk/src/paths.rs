//! Where Inlay keeps its files.
use std::path::PathBuf;

/// %APPDATA%-style folder for config.toml and fonts/: ~/Library/Application Support/inlay
/// on macOS, $XDG_CONFIG_HOME/inlay (~/.config/inlay) on Linux.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("inlay"))
}

/// The config file, same format as the Windows app's.
pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

/// Where user fonts go.
pub fn fonts_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("fonts"))
}

/// The lyrics cache: ~/Library/Caches/inlay on macOS, ~/.cache/inlay on Linux.
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("inlay"))
}
