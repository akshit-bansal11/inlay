//! The fonts the config names, made available to egui by family name.
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use fontdb::{Database, Family, Query, Weight};
use inlay_core::config::{ClockRow, Config};

/// The names `install` found a font for.
static LOADED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The system fonts, scanned once: the scan is slow and they do not change while running.
fn system_db() -> &'static Database {
    static DB: OnceLock<Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = Database::new();
        db.load_system_fonts();
        db
    })
}

/// The bytes and face index of the face of `name` closest to `weight`.
fn face(db: &Database, name: &str, weight: u32) -> Option<(Vec<u8>, u32)> {
    let weight = Weight(u16::try_from(weight).unwrap_or(400));
    let id = db.query(&Query {
        families: &[Family::Name(name)],
        weight,
        ..Query::default()
    })?;
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}

/// Makes the fonts the config names available to egui, by family name: system fonts
/// (fontdb), plus .ttf/.otf files in the fonts folder, which win. A name with no font
/// falls back to egui's default. Called again after the config changes.
pub fn install(ctx: &Context, cfg: &Config, fonts_dir: Option<&Path>) {
    // Every named font with the weight of the first label that asks for it.
    let mut wanted: Vec<(&str, u32)> = vec![(cfg.theme.font.as_str(), 400)];
    for row in ClockRow::ALL {
        let label = cfg.clock.label(row);
        wanted.push((label.font.as_str(), label.weight));
    }
    for label in [
        &cfg.player.title,
        &cfg.player.artist,
        &cfg.player.elapsed,
        &cfg.player.total,
    ] {
        wanted.push((label.font.as_str(), label.weight));
    }
    wanted.push((cfg.lyrics.font.as_str(), cfg.lyrics.weight));

    let user_db = fonts_dir.map(|dir| {
        let mut db = Database::new();
        db.load_fonts_dir(dir);
        db
    });

    let mut defs = FontDefinitions::default();
    let fallbacks = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut loaded = BTreeSet::new();
    for (name, weight) in wanted {
        if name.is_empty() || loaded.contains(name) {
            continue;
        }
        // ponytail: one face per name, the closest to the first weight asked for; the
        // weight of the other labels and the letter spacing are not applied. Upgrade
        // path: one family per (name, weight) pair.
        let found = user_db
            .as_ref()
            .and_then(|db| face(db, name, weight))
            .or_else(|| face(system_db(), name, weight));
        let Some((bytes, index)) = found else {
            continue;
        };
        let mut data = FontData::from_owned(bytes);
        data.index = index;
        defs.font_data
            .insert(name.to_owned(), std::sync::Arc::new(data));
        let mut chain = vec![name.to_owned()];
        chain.extend(fallbacks.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), chain);
        loaded.insert(name.to_owned());
    }
    ctx.set_fonts(defs);
    *LOADED.lock().unwrap_or_else(PoisonError::into_inner) = loaded;
}

/// The egui family for a configured font name: the named family when `install` loaded
/// it, else the theme font's, else the default proportional family.
pub fn family(name: &str, theme_font: &str) -> FontFamily {
    let loaded = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    [name, theme_font]
        .into_iter()
        .find(|n| loaded.contains(*n))
        .map_or(FontFamily::Proportional, |n| FontFamily::Name(n.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unloaded_name_is_the_default_family() {
        assert_eq!(family("No Such Font", ""), FontFamily::Proportional);
    }
}
