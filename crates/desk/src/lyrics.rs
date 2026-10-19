//! Fetches lyrics on a short-lived thread, with a disk cache so a track is
//! only ever downloaded once. LRCLIB answers first, with line timing; a
//! LyricsPlus server then replaces that with word timing when it has any.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inlay_core::lrc::{parse_lrc, parse_plain};
use inlay_core::lrclib::{self, Query, Record};
use inlay_core::lyricsplus;
use inlay_core::timing::{Line, normalize_lines};

use crate::model::{Lyrics, LyricsState};

const USER_AGENT: &str = concat!(
    "Inlay/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/akshit-bansal11/inlay)"
);
/// A "no lyrics" answer is asked again after this long: LRCLIB gains tracks.
const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A LyricsPlus server answers two requests in ten seconds and refuses the
/// third, so requests to one server are kept this far apart.
const WORD_REQUEST_GAP: Duration = Duration::from_millis(5200);
const TOO_MANY_REQUESTS: u16 = 429;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A bigger answer than this is not lyrics.
const MAX_BODY: u64 = 2 * 1024 * 1024;

/// Looks a track up on a short-lived thread and publishes what it finds into `shared`,
/// unless the track changed meanwhile (`track_gen` no longer matches). `word_servers` is
/// `None` when word timing is off. `wake` is called after each publish so the window redraws.
pub fn fetch(
    query: Query,
    track_gen: u64,
    shared: Arc<Mutex<LyricsState>>,
    word_servers: Option<Vec<String>>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::spawn(move || {
        let publish = |lyrics: Lyrics| {
            if let Ok(mut state) = shared.lock()
                && state.track_gen == track_gen
            {
                state.lyrics = lyrics;
            }
            wake();
        };
        let Some(servers) = word_servers else {
            return publish(lookup(&query));
        };
        let path = cache_path(&query, "words.json");
        let known = path
            .as_deref()
            .and_then(|p| cached_words(p, query.duration_ms));
        if let Some(Some(lines)) = known {
            return publish(Lyrics::Synced(lines));
        }
        // The servers can be slow, so the line-timed answer goes up first.
        publish(lookup(&query));
        // A track that was skipped past is not worth one of the few requests.
        let wanted = || {
            shared
                .lock()
                .is_ok_and(|state| state.track_gen == track_gen)
        };
        if known.is_none()
            && let Some(lines) = download_words(&query, &servers, path.as_deref(), &wanted)
        {
            publish(Lyrics::Synced(lines));
        }
    });
}

/// Finds lyrics for a track: the disk cache first, LRCLIB otherwise.
fn lookup(query: &Query) -> Lyrics {
    let path = cache_path(query, "json");
    let record = match path.as_deref().and_then(read_cache) {
        Some(known) => known,
        None => match download(query) {
            Some(answer) => {
                if let Some(path) = &path {
                    write_cache(path, &answer);
                }
                answer
            }
            // Offline or LRCLIB is down: nothing is cached, so the next try asks again.
            None => return Lyrics::Unavailable,
        },
    };
    to_lyrics(record, query.duration_ms)
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .user_agent(USER_AGENT)
            // A 404 or 429 is an answer to read, not an error.
            .http_status_as_error(false)
            .build(),
    )
}

/// Status code and body, or `None` when the request never completed or the
/// body was over `MAX_BODY`.
fn get(agent: &ureq::Agent, url: &str) -> Option<(u16, String)> {
    let mut response = match agent.get(url).call() {
        Ok(response) => response,
        Err(error) => {
            eprintln!("lyrics request failed: {error}");
            return None;
        }
    };
    let status = response.status().as_u16();
    let mut body = String::new();
    let read = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .reader()
        .read_to_string(&mut body);
    match read {
        Ok(_) => Some((status, body)),
        Err(error) => {
            eprintln!("lyrics body not read: {error}");
            None
        }
    }
}

/// A request to a LyricsPlus server, held back until that server's last
/// one is `WORD_REQUEST_GAP` old.
fn paced_get(agent: &ureq::Agent, server: &str, url: &str) -> Option<(u16, String)> {
    static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
    // The lock is not held while sleeping or requesting.
    // ponytail: two threads can occasionally both pass for one server; a per-server slot fixes it.
    let wait = LAST
        .lock()
        .ok()?
        .get_or_insert_with(HashMap::new)
        .get(server)
        .and_then(|at| WORD_REQUEST_GAP.checked_sub(at.elapsed()));
    if let Some(wait) = wait {
        std::thread::sleep(wait);
    }
    let answer = get(agent, url);
    LAST.lock()
        .ok()?
        .get_or_insert_with(HashMap::new)
        .insert(server.to_owned(), Instant::now());
    answer
}

/// `Some(None)` is a remembered miss, stored as an empty file.
fn cached_words(path: &Path, duration_ms: i64) -> Option<Option<Vec<Line>>> {
    let body = std::fs::read_to_string(path).ok()?;
    if body.is_empty() {
        let age = std::fs::metadata(path).ok()?.modified().ok()?.elapsed();
        return age.is_ok_and(|age| age <= MISS_TTL).then_some(None);
    }
    // A body that no longer parses is asked for again.
    lyricsplus::parse(&body, duration_ms).map(Some)
}

/// Asks each server in turn, the cleaned title first: it is the one the
/// servers usually know. A miss is only remembered when a server said so;
/// servers that are down or rate-limited say nothing about the track. A
/// server that refuses for asking too often is asked once more.
fn download_words(
    query: &Query,
    servers: &[String],
    path: Option<&Path>,
    wanted: &dyn Fn() -> bool,
) -> Option<Vec<Line>> {
    let agent = agent();
    let mut missing = false;
    for attempt in lrclib::attempts(query).iter().rev() {
        'servers: for server in servers {
            let url = lyricsplus::url(server, attempt);
            for _ in 0..2 {
                if !wanted() {
                    return None;
                }
                match paced_get(&agent, server, &url) {
                    Some((200, body)) => {
                        if let Some(lines) = lyricsplus::parse(&body, query.duration_ms) {
                            if let Some(path) = path {
                                write_text(path, &body);
                            }
                            return Some(lines);
                        }
                        // Line-timed only, or another recording of the song.
                        missing = true;
                        break 'servers;
                    }
                    Some((404, _)) => {
                        missing = true;
                        break 'servers;
                    }
                    Some((TOO_MANY_REQUESTS, _)) => {}
                    _ => break,
                }
            }
        }
    }
    if missing && let Some(path) = path {
        write_text(path, "");
    }
    None
}

fn to_lyrics(record: Option<Record>, duration_ms: i64) -> Lyrics {
    let Some(record) = record else {
        return Lyrics::Missing;
    };
    if let Some(synced) = &record.synced_lyrics {
        let lines = normalize_lines(&parse_lrc(synced).lines, duration_ms);
        if !lines.is_empty() {
            return Lyrics::Synced(lines);
        }
    }
    if record.instrumental {
        return Lyrics::Instrumental;
    }
    match record.plain_lyrics.as_deref().map(parse_plain) {
        Some(lines) if !lines.is_empty() => Lyrics::Plain(lines),
        _ => Lyrics::Missing,
    }
}

/// FNV-1a. The standard hasher is not guaranteed stable across Rust
/// releases, and these names are file names that must survive an update.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// `None` when the platform has no cache directory: nothing is cached then.
fn cache_path(q: &Query, extension: &str) -> Option<PathBuf> {
    crate::paths::cache_dir().map(|dir| cache_path_in(&dir, q, extension))
}

fn cache_path_in(dir: &Path, q: &Query, extension: &str) -> PathBuf {
    let key = format!(
        "{}\n{}\n{}\n{}",
        q.title,
        q.artist,
        q.album,
        q.duration_ms / 1000
    );
    dir.join("lyrics")
        .join(format!("{:016x}.{extension}", fnv1a(&key)))
}

/// `Some(None)` is a remembered miss.
fn read_cache(path: &Path) -> Option<Option<Record>> {
    let text = std::fs::read_to_string(path).ok()?;
    let cached: Option<Record> = serde_json::from_str(&text).ok()?;
    if cached.is_none() {
        let age = std::fs::metadata(path).ok()?.modified().ok()?.elapsed();
        if age.is_ok_and(|age| age > MISS_TTL) {
            return None;
        }
    }
    Some(cached)
}

fn write_cache(path: &Path, record: &Option<Record>) {
    write_text(path, &serde_json::to_string(record).unwrap_or_default());
}

fn write_text(path: &Path, text: &str) {
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, text));
    if let Err(error) = written {
        eprintln!("could not cache lyrics: {error}");
    }
}

/// `Some(None)` means LRCLIB answered and has nothing; `None` means it could
/// not be reached, which must not be cached as a miss.
fn download(query: &Query) -> Option<Option<Record>> {
    let agent = agent();

    let attempts = lrclib::attempts(query);
    for attempt in &attempts {
        match get(&agent, &lrclib::get_url(attempt))? {
            (200, body) => {
                if let Ok(record) = serde_json::from_str::<Record>(&body) {
                    return Some(Some(record));
                }
            }
            // 404 is LRCLIB's ordinary "no match", not a failure.
            (404, _) => {}
            _ => return None,
        }
    }

    match get(&agent, &lrclib::search_url(attempts.last()?))? {
        (200, body) => {
            let records = serde_json::from_str::<Vec<Record>>(&body).ok()?;
            Some(lrclib::pick_best(records, query.duration_ms))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(synced: Option<&str>, plain: Option<&str>, instrumental: bool) -> Option<Record> {
        Some(Record {
            duration: Some(100.0),
            instrumental,
            plain_lyrics: plain.map(str::to_owned),
            synced_lyrics: synced.map(str::to_owned),
        })
    }

    #[test]
    fn prefers_synced_lyrics_then_plain_then_missing() {
        let synced = to_lyrics(record(Some("[00:01.00] hi"), Some("hi"), false), 100_000);
        assert!(matches!(synced, Lyrics::Synced(lines) if lines.len() == 1));

        let plain = to_lyrics(
            record(Some("no timestamps"), Some("hi\nthere"), false),
            100_000,
        );
        assert!(matches!(plain, Lyrics::Plain(lines) if lines.len() == 2));

        assert!(matches!(
            to_lyrics(record(None, None, true), 100_000),
            Lyrics::Instrumental
        ));
        assert!(matches!(
            to_lyrics(record(None, Some("  "), false), 100_000),
            Lyrics::Missing
        ));
        assert!(matches!(to_lyrics(None, 100_000), Lyrics::Missing));
    }

    #[test]
    fn the_cache_name_is_stable() {
        // FNV-1a test vector; these are file names and must never change.
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn a_cached_miss_older_than_the_ttl_is_asked_again() {
        let dir = std::env::temp_dir().join(format!("inlay-desk-lyrics-{}", std::process::id()));
        let query = Query {
            title: "t".into(),
            artist: "a".into(),
            album: "b".into(),
            duration_ms: 100_000,
        };
        let path = cache_path_in(&dir, &query, "json");
        write_cache(&path, &None);
        assert!(matches!(read_cache(&path), Some(None)));

        let old = std::time::SystemTime::now() - MISS_TTL - Duration::from_secs(60);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(old).unwrap();
        drop(file);
        assert!(read_cache(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
