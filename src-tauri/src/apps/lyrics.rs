//! Lyrics for the Music app, from LRCLIB (https://lrclib.net): an open
//! lyrics database with no key or account. What is sent is the song's title,
//! artist, album and length, and only while lyrics are on screen: the
//! expanded Music view, Idle Mode, or the pill when Settings › Apps › Music ›
//! Show Lyrics in the Pill is on. Answers are cached in the `lyrics` table,
//! misses for a few days.
//!
//! Word-synced records (`hasWordSync`) also carry a `lyricsfile` with every
//! word's start, read by `lyricsfile.rs`. The UI parses the LRC text itself
//! (`src/apps/music/lyrics.ts`) and prefers word times when there are some.

use super::lyricsfile::{self, WordLine};
use crate::db::Db;
use crate::http::{get_optional, percent_encode};
use crate::time::now_ms;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

const API: &str = "https://lrclib.net/api";
/// A song LRCLIB didn't have is asked for again after this long.
const MISS_TTL_MS: i64 = 3 * 24 * 60 * 60 * 1000;
const CACHE_ROWS: i64 = 2000;
/// Records whose length is further off than this are another version.
const MAX_DURATION_DIFF_S: f64 = 4.0;
/// A word-synced record this far off in length can still be the same
/// recording (players and uploaders disagree about trailing silence), if its
/// line times agree with the closest record's.
const WORD_SYNC_DURATION_DIFF_S: f64 = 15.0;
/// How many leading lines are compared, and how far apart they may be on average.
const ALIGN_LINES: usize = 8;
const ALIGN_MAX_MS: f64 = 600.0;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub found: bool,
    pub instrumental: bool,
    /// LRC text with `[mm:ss.xx]` line times.
    pub synced: Option<String>,
    pub plain: Option<String>,
    /// Every word's start, when the record is word-synced.
    pub word_lines: Option<Vec<WordLine>>,
    /// The `lyricsfile` the word times came from, as cached.
    #[serde(skip)]
    pub lyricsfile: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Record {
    duration: Option<f64>,
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
    has_word_sync: bool,
    lyricsfile: Option<String>,
}

impl Record {
    fn has_text(&self) -> bool {
        let filled = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.trim().is_empty());
        filled(&self.synced_lyrics) || filled(&self.plain_lyrics)
    }

    fn usable(&self) -> bool {
        self.instrumental || self.has_text()
    }

    fn into_lyrics(self) -> Lyrics {
        let keep = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
        let instrumental = self.instrumental && !self.has_text();
        let file = if self.has_word_sync { keep(self.lyricsfile) } else { None };
        let word_lines = file.as_deref().and_then(lyricsfile::parse);
        Lyrics {
            found: true,
            instrumental,
            synced: keep(self.synced_lyrics),
            plain: keep(self.plain_lyrics),
            // Only keep a file that could be read.
            lyricsfile: file.filter(|_| word_lines.is_some()),
            word_lines,
        }
    }
}

pub struct Query {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_s: Option<f64>,
}

/// Words in brackets that are about the upload, not the song.
const NOISE: &[&str] = &[
    "official", "video", "audio", "lyric", "visuali", "remaster", "hd", "4k", "mv", "m/v", "explicit", "clip",
    "feat", "ft.", "with ", "prod.", "color coded", "sözleri", "klip",
];
/// " - …" endings that name an edition rather than another song.
const EDITION: &[&str] = &["remaster", "radio edit", "mono", "stereo", "single version", "bonus track", "from "];

/// Title without upload noise: "(Official Video)", "[4K]", "- 2011 Remaster",
/// "feat. X".
pub fn clean_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[start] == b'(' { ')' } else { ']' };
        let Some(len) = rest[start..].find(close) else { break };
        let inner = rest[start + 1..start + len].to_lowercase();
        out.push_str(&rest[..start]);
        if !NOISE.iter().any(|n| inner.contains(n)) {
            out.push_str(&rest[start..=start + len]);
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);

    if let Some(i) = out.rfind(" - ")
        && EDITION.iter().any(|e| out[i..].to_lowercase().contains(e))
    {
        out.truncate(i);
    }
    let lower = out.to_lowercase();
    if let Some(i) = [" feat. ", " ft. ", " feat "].iter().filter_map(|m| lower.find(m)).min() {
        out.truncate(i);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The main artist: "X - Topic" and "XVEVO" channels, and only the first of
/// several ("A, B", "A feat. B"). "A & B" is often one act, so it stays.
pub fn clean_artist(artist: &str) -> String {
    let mut a = artist.trim();
    a = a.strip_suffix(" - Topic").unwrap_or(a);
    a = a.strip_suffix("VEVO").unwrap_or(a);
    let lower = a.to_lowercase();
    let cut = [", ", " feat. ", " ft. ", " featuring ", "; "]
        .iter()
        .filter_map(|m| lower.find(m))
        .min()
        .unwrap_or(a.len());
    a[..cut].trim().to_string()
}

/// "Artist - Title" as browsers report a music video (the artist field is
/// then the channel's name).
pub fn split_video_title(title: &str) -> Option<(String, String)> {
    for sep in [" - ", " – ", " — ", " | "] {
        if let Some((a, t)) = title.split_once(sep) {
            let (a, t) = (a.trim(), t.trim());
            if !a.is_empty() && !t.is_empty() {
                return Some((a.to_string(), t.to_string()));
            }
        }
    }
    None
}

/// (artist, title) pairs to search for, best guess first.
fn candidates(q: &Query) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |a: String, t: String| {
        if !t.is_empty() && !out.iter().any(|(x, y)| x.eq_ignore_ascii_case(&a) && y.eq_ignore_ascii_case(&t)) {
            out.push((a, t));
        }
    };
    push(clean_artist(&q.artist), clean_title(&q.title));
    if let Some((a, t)) = split_video_title(&q.title) {
        push(clean_artist(&a), clean_title(&t));
    }
    out
}

pub fn cache_key(q: &Query) -> String {
    let (a, t) = candidates(q).into_iter().next().unwrap_or_default();
    format!("{}|{}", a.to_lowercase(), t.to_lowercase())
}

fn duration_diff(r: &Record, duration_s: Option<f64>) -> f64 {
    match (duration_s, r.duration) {
        (Some(want), Some(got)) => (want - got).abs(),
        _ => 0.0,
    }
}

/// The record that fits best: right length, then word-synced over
/// line-synced over plain.
fn best(records: &[Record], duration_s: Option<f64>) -> Option<Record> {
    let diff = |r: &Record| duration_diff(r, duration_s);
    records
        .iter()
        .filter(|r| r.usable() && diff(r) <= MAX_DURATION_DIFF_S)
        .min_by(|a, b| {
            let rank = |r: &Record| (r.synced_lyrics.is_none(), !r.has_word_sync, !r.has_text());
            rank(a).cmp(&rank(b)).then(diff(a).total_cmp(&diff(b)))
        })
        .cloned()
}

/// Start times of the first LRC lines, in ms.
fn line_times(lrc: &str) -> Vec<i64> {
    lrc.lines()
        .filter_map(|l| {
            let tag = l.trim().strip_prefix('[')?.split_once(']')?.0;
            let (m, s) = tag.split_once(':')?;
            Some((m.parse::<f64>().ok()? * 60_000.0 + s.parse::<f64>().ok()? * 1000.0).round() as i64)
        })
        .take(ALIGN_LINES)
        .collect()
}

/// Average distance between the first line times, if both have enough lines.
fn misalignment(a: &str, b: &str) -> Option<f64> {
    let (a, b) = (line_times(a), line_times(b));
    let n = a.len().min(b.len());
    (n >= 3).then(|| a.iter().zip(&b).map(|(x, y)| (x - y).abs() as f64).sum::<f64>() / n as f64)
}

/// `chosen`, or a word-synced record of the same recording: one a little
/// further off in length whose line times match `chosen`'s.
fn with_word_sync(chosen: Record, records: &[Record], duration_s: Option<f64>) -> Record {
    let Some(base) = chosen.synced_lyrics.as_deref().filter(|_| !chosen.has_word_sync) else { return chosen };
    records
        .iter()
        .filter(|r| r.has_word_sync && duration_diff(r, duration_s) <= WORD_SYNC_DURATION_DIFF_S)
        .filter_map(|r| Some((misalignment(base, r.synced_lyrics.as_deref()?)?, r)))
        .filter(|(off, _)| *off <= ALIGN_MAX_MS)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, r)| r.clone())
        .unwrap_or(chosen)
}

fn get_url(q: &Query, duration_s: f64) -> String {
    format!(
        "{API}/get?artist_name={}&track_name={}&album_name={}&duration={}",
        percent_encode(&q.artist),
        percent_encode(&q.title),
        percent_encode(&q.album),
        duration_s.round() as i64
    )
}

fn search_url(artist: &str, title: &str) -> String {
    if artist.is_empty() {
        format!("{API}/search?track_name={}", percent_encode(title))
    } else {
        format!("{API}/search?track_name={}&artist_name={}", percent_encode(title), percent_encode(artist))
    }
}

fn query_url(text: &str) -> String {
    format!("{API}/search?q={}", percent_encode(text))
}

/// Asks LRCLIB: the exact lookup first (it needs the length), then searches.
pub fn fetch(q: &Query, http: impl Fn(&str) -> Result<Option<String>, String>) -> Result<Lyrics, String> {
    let parse_one = |body: String| serde_json::from_str::<Record>(&body).map_err(|e| e.to_string());
    let parse_many = |body: Option<String>| -> Result<Vec<Record>, String> {
        body.map_or(Ok(Vec::new()), |b| serde_json::from_str(&b).map_err(|e| e.to_string()))
    };

    // The exact match, when it is word-synced already, is the answer. If it
    // is only line-synced, a search may still find the same recording with
    // word times, so it waits as the fallback.
    let mut exact = None;
    if let Some(d) = q.duration_s
        && !q.artist.is_empty()
        && let Some(body) = http(&get_url(q, d))?
    {
        let record = parse_one(body)?;
        if record.usable() {
            if record.has_word_sync || record.synced_lyrics.is_none() {
                return Ok(record.into_lyrics());
            }
            exact = Some(record);
        }
    }
    let candidates = candidates(q);
    for (artist, title) in &candidates {
        let records = parse_many(http(&search_url(artist, title))?)?;
        if let Some(r) = exact.take().or_else(|| best(&records, q.duration_s)) {
            return Ok(with_word_sync(r, &records, q.duration_s).into_lyrics());
        }
    }
    if let Some((artist, title)) = candidates.first()
        && !artist.is_empty()
    {
        let records = parse_many(http(&query_url(&format!("{artist} {title}")))?)?;
        if let Some(r) = best(&records, q.duration_s) {
            return Ok(with_word_sync(r, &records, q.duration_s).into_lyrics());
        }
    }
    Ok(exact.map(Record::into_lyrics).unwrap_or_default())
}

fn cached(db: &Db, key: &str) -> Option<Lyrics> {
    db.with_conn(|c| {
        c.query_row(
            "SELECT found, instrumental, synced, plain, fetched_at, words FROM lyrics WHERE key = ?1",
            params![key],
            |r| {
                let file: Option<String> = r.get(5)?;
                let lyrics = Lyrics {
                    found: r.get(0)?,
                    instrumental: r.get(1)?,
                    synced: r.get(2)?,
                    plain: r.get(3)?,
                    word_lines: file.as_deref().and_then(lyricsfile::parse),
                    lyricsfile: file,
                };
                Ok((lyrics, r.get::<_, i64>(4)?))
            },
        )
        .optional()
    })
    .ok()
    .flatten()
    .filter(|(l, at)| l.found || now_ms() - at < MISS_TTL_MS)
    .map(|(l, _)| l)
}

fn store(db: &Db, key: &str, l: &Lyrics) {
    let result = db.with_conn(|c| {
        c.execute(
            "INSERT OR REPLACE INTO lyrics (key, found, instrumental, synced, plain, fetched_at, words)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![key, l.found, l.instrumental, l.synced, l.plain, now_ms(), l.lyricsfile],
        )?;
        c.execute(
            "DELETE FROM lyrics WHERE key NOT IN (SELECT key FROM lyrics ORDER BY fetched_at DESC LIMIT ?1)",
            params![CACHE_ROWS],
        )
    });
    if let Err(e) = result {
        log::warn!("could not cache lyrics: {e}");
    }
}

pub fn lookup(db: &Db, q: &Query) -> Result<Lyrics, String> {
    let key = cache_key(q);
    if key == "|" {
        return Ok(Lyrics::default());
    }
    if let Some(l) = cached(db, &key) {
        return Ok(l);
    }
    // Network errors are not cached: the next try may work.
    let lyrics = fetch(q, get_optional)?;
    store(db, &key, &lyrics);
    Ok(lyrics)
}

#[tauri::command]
pub async fn lyrics_get(
    db: State<'_, Arc<Db>>,
    artist: String,
    title: String,
    album: String,
    duration_ms: i64,
) -> Result<Lyrics, String> {
    let db = db.inner().clone();
    let q = Query { artist, title, album, duration_s: (duration_ms > 0).then(|| duration_ms as f64 / 1000.0) };
    tauri::async_runtime::spawn_blocking(move || lookup(&db, &q)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn q(artist: &str, title: &str, duration_s: Option<f64>) -> Query {
        Query { artist: artist.into(), title: title.into(), album: String::new(), duration_s }
    }

    #[test]
    fn titles_lose_upload_noise() {
        assert_eq!(clean_title("Bohemian Rhapsody (Official Video Remastered)"), "Bohemian Rhapsody");
        assert_eq!(clean_title("Here Comes The Sun - Remastered 2009"), "Here Comes The Sun");
        assert_eq!(clean_title("Blinding Lights [4K]  "), "Blinding Lights");
        assert_eq!(clean_title("Stay (feat. Justin Bieber)"), "Stay");
        assert_eq!(clean_title("Señorita ft. Camila Cabello"), "Señorita");
        assert_eq!(clean_title("Gel Gör Beni (Klip)"), "Gel Gör Beni");
        // Not noise: part of the song's name, and real remixes.
        assert_eq!(clean_title("Lose Yourself (Live)"), "Lose Yourself (Live)");
        assert_eq!(clean_title("Titanium - David Guetta Remix"), "Titanium - David Guetta Remix");
        assert_eq!(clean_title("Unclosed (bracket"), "Unclosed (bracket");
    }

    #[test]
    fn artists_lose_channel_suffixes_and_guests() {
        assert_eq!(clean_artist("Tarkan - Topic"), "Tarkan");
        assert_eq!(clean_artist("TaylorSwiftVEVO"), "TaylorSwift");
        assert_eq!(clean_artist("Daft Punk, Pharrell Williams"), "Daft Punk");
        assert_eq!(clean_artist("Simon & Garfunkel"), "Simon & Garfunkel");
        assert_eq!(clean_artist("Sezen Aksu"), "Sezen Aksu");
    }

    #[test]
    fn video_titles_split_into_artist_and_title() {
        assert_eq!(split_video_title("Queen - Bohemian Rhapsody"), Some(("Queen".into(), "Bohemian Rhapsody".into())));
        assert_eq!(split_video_title("Just A Title"), None);
        assert_eq!(split_video_title(" - nothing"), None);
        let c = candidates(&q("QueenOfficial", "Queen - Bohemian Rhapsody (Official Video)", None));
        assert_eq!(c[1], ("Queen".into(), "Bohemian Rhapsody".into()));
    }

    #[test]
    fn best_prefers_synced_and_the_right_length() {
        let rec = |d: f64, synced: bool| Record {
            duration: Some(d),
            synced_lyrics: synced.then(|| "[00:01.00] a".into()),
            plain_lyrics: Some("a".into()),
            ..Default::default()
        };
        let picked = best(&[rec(200.0, false), rec(201.0, true), rec(300.0, true)], Some(200.0)).unwrap();
        assert_eq!(picked.duration, Some(201.0));
        assert!(best(&[rec(300.0, true)], Some(200.0)).is_none());
        assert!(best(&[Record::default()], None).is_none(), "empty records are skipped");
    }

    #[test]
    fn fetch_falls_back_to_search() {
        let asked = RefCell::new(Vec::new());
        let http = |url: &str| {
            asked.borrow_mut().push(url.to_string());
            Ok(if url.contains("/get?") {
                None
            } else if url.contains("track_name=Bohemian%20Rhapsody&artist_name=Queen") {
                Some(r#"[{"duration": 355, "instrumental": false, "plainLyrics": "Is this", "syncedLyrics": "[00:00.50] Is this"}]"#.into())
            } else {
                Some("[]".into())
            })
        };
        let found = fetch(&q("QueenOfficial", "Queen - Bohemian Rhapsody (Official Video)", Some(356.0)), http).unwrap();
        assert!(found.found);
        assert_eq!(found.synced.as_deref(), Some("[00:00.50] Is this"));
        assert_eq!(asked.borrow().len(), 3, "get, first search, split-title search");
    }

    #[test]
    fn word_synced_records_win_and_keep_their_words() {
        let file = "lines:
- text: hey you
  start_ms: 1000
  end_ms: 2000
  words:
  - text: 'hey '
    start_ms: 1000
  - text: you
    start_ms: 1400
";
        let body = format!(
            r#"[{{"duration": 100, "syncedLyrics": "[00:01.00] hey you", "hasWordSync": false}},
               {{"duration": 100, "syncedLyrics": "[00:01.00] hey you", "hasWordSync": true, "lyricsfile": {}}}]"#,
            serde_json::to_string(file).unwrap()
        );
        let http = |u: &str| Ok(if u.contains("/get?") { None } else { Some(body.clone()) });
        let l = fetch(&q("A", "B", Some(100.0)), http).unwrap();
        let words = l.word_lines.as_ref().expect("word lines");
        assert_eq!(words[0].words[1].start_ms, 1400);

        let db = Db::open_in_memory().unwrap();
        store(&db, "a|b", &l);
        assert_eq!(cached(&db, "a|b").unwrap().word_lines, l.word_lines, "word times survive the cache");
    }

    #[test]
    fn word_sync_from_the_same_recording_a_bit_longer() {
        // The player says 222.8 s; the word-synced record says 229 s (trailing
        // silence), with the same line times. A live version is word-synced
        // too, but its lines are elsewhere.
        let lrc = "[00:30.55] a\n[00:37.26] b\n[00:43.32] c\n[00:49.75] d";
        let file = "lines:\n- text: a\n  start_ms: 30550\n  words:\n  - text: a\n    start_ms: 30550\n";
        let rec = |d: f64, synced: &str, words: bool| Record {
            duration: Some(d),
            synced_lyrics: Some(synced.into()),
            plain_lyrics: Some("a".into()),
            has_word_sync: words,
            lyricsfile: words.then(|| file.into()),
            ..Default::default()
        };
        let records = [
            rec(223.0, "[00:30.49] a\n[00:36.93] b\n[00:43.28] c\n[00:49.71] d", false),
            rec(229.0, lrc, true),
            rec(235.0, "[00:40.00] a\n[00:47.00] b\n[00:53.00] c\n[00:59.00] d", true),
        ];
        let chosen = best(&records, Some(222.8)).unwrap();
        assert!(!chosen.has_word_sync, "only the 223 s record is close enough");
        let upgraded = with_word_sync(chosen, &records, Some(222.8));
        assert_eq!(upgraded.duration, Some(229.0));
        // Nothing aligned: keep what was chosen.
        let lone = with_word_sync(records[0].clone(), &records[2..], Some(222.8));
        assert_eq!(lone.duration, Some(223.0));
    }

    #[test]
    fn instrumental_and_misses() {
        let instrumental = |_: &str| Ok(Some(r#"{"duration": 100, "instrumental": true}"#.into()));
        let l = fetch(&q("A", "B", Some(100.0)), instrumental).unwrap();
        assert!(l.found && l.instrumental);
        let nothing = |u: &str| Ok(if u.contains("/get?") { None } else { Some("[]".into()) });
        assert_eq!(fetch(&q("A", "B", Some(100.0)), nothing).unwrap(), Lyrics::default());
    }

    #[test]
    fn cache_keeps_hits_and_expires_misses() {
        let db = Db::open_in_memory().unwrap();
        let hit = Lyrics { found: true, plain: Some("la".into()), ..Default::default() };
        store(&db, "a|b", &hit);
        assert_eq!(cached(&db, "a|b"), Some(hit));
        store(&db, "c|d", &Lyrics::default());
        assert_eq!(cached(&db, "c|d"), Some(Lyrics::default()));
        db.with_conn(|c| c.execute("UPDATE lyrics SET fetched_at = 0", [])).unwrap();
        assert_eq!(cached(&db, "c|d"), None, "an old miss is asked again");
        assert!(cached(&db, "a|b").is_some(), "a hit never expires");
    }

    #[test]
    #[ignore]
    fn live_lrclib() {
        let l = fetch(&q("Queen", "Bohemian Rhapsody", Some(355.0)), get_optional).unwrap();
        assert!(l.found && l.synced.is_some(), "{l:?}");
        // Spotify reports 222.8 s for this one; LRCLIB's word-synced record is 229 s.
        let mut dans = q("Hayko Cepkin", "Dans Et", Some(222.8));
        dans.album = "Kabul Olur / Dans Et".into();
        let words = fetch(&dans, get_optional).unwrap();
        assert!(words.word_lines.is_some_and(|w| w.len() > 10), "Dans Et is word-synced on LRCLIB");
    }
}
