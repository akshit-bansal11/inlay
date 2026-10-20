//! The config file, shared in format with the Windows app.
use std::path::PathBuf;
use std::time::SystemTime;

use inlay_core::config::Config;

use crate::paths;

/// The loaded config and when its file last changed, so an edit made by hand is
/// picked up without a restart.
pub struct Settings {
    pub cfg: Config,
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
}

impl Settings {
    /// Reads config.toml, writing the defaults first when there is none, so there is a
    /// file to edit. A file that does not parse keeps the defaults and is left alone.
    pub fn load() -> Self {
        let mut s = Self {
            cfg: Config::default(),
            path: paths::config_file(),
            modified: None,
        };
        if let Some(p) = &s.path
            && !p.exists()
        {
            if let Some(dir) = p.parent()
                && let Err(e) = std::fs::create_dir_all(dir)
            {
                eprintln!("inlay: cannot create {}: {e}", dir.display());
            }
            if let Err(e) = std::fs::write(p, s.cfg.to_toml()) {
                eprintln!("inlay: cannot write {}: {e}", p.display());
            }
        }
        s.reload_if_changed();
        s
    }

    /// Re-reads the file if it changed since the last read. True when `cfg` changed.
    pub fn reload_if_changed(&mut self) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) else {
            return false;
        };
        if self.modified == Some(modified) {
            return false;
        }
        // Stamp even a bad file, so its error prints once per edit, not every poll.
        self.modified = Some(modified);
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("inlay: cannot read {}: {e}", path.display());
                return false;
            }
        };
        match Config::from_toml(&text) {
            Ok(cfg) if cfg != self.cfg => {
                self.cfg = cfg;
                true
            }
            Ok(_) => false,
            Err(e) => {
                eprintln!(
                    "inlay: {} does not parse, keeping the last config: {e}",
                    path.display()
                );
                false
            }
        }
    }
}
