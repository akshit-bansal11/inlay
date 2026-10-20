//! Album art download.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::ColorImage;

use crate::player::decode_art;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A bigger answer than this is not cover art.
const MAX_BODY: u64 = 2 * 1024 * 1024;

/// Downloads `url` on a short-lived thread, decodes it and stores `(url, image)` in `out`,
/// then calls `wake`. Only https URLs are fetched. The url comes back with the image so
/// the caller can drop art that belongs to a track already skipped.
pub fn fetch(
    url: String,
    out: Arc<Mutex<Option<(String, ColorImage)>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    if !url.starts_with("https://") {
        eprintln!("art: skipping non-https url {url}");
        return;
    }
    std::thread::spawn(move || {
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .build(),
        );
        let bytes = match agent.get(&url).call().and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(MAX_BODY)
                .read_to_vec()
        }) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("art: download failed: {error}");
                return;
            }
        };
        let Some(image) = decode_art(&bytes) else {
            eprintln!("art: not a PNG or JPEG");
            return;
        };
        if let Ok(mut slot) = out.lock() {
            *slot = Some((url, image));
        }
        wake();
    });
}
