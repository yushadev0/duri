//! Music app: reads and controls whatever is playing through Windows'
//! Global System Media Transport Controls (Spotify, browsers, Media Player…).
//!
//! A single worker thread owns the WinRT session manager. Commands from the
//! frontend are sent to it over a channel; between commands it polls the
//! current session every 500 ms and emits `music://state` when anything
//! changes and `music://artwork` when the track changes.

use crate::time::now_ms;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::DataReader;

/// 100 ns ticks between 1601-01-01 (WinRT DateTime) and 1970-01-01.
const WINRT_EPOCH_OFFSET: i64 = 116_444_736_000_000_000;
const POLL: Duration = Duration::from_millis(500);
/// Some players publish the thumbnail a moment after the title.
const ARTWORK_ATTEMPTS: u8 = 8;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MusicState {
    pub available: bool,
    pub source_app_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    /// Unix ms at which `position_ms` was measured; the UI extrapolates from it.
    pub position_updated_at: i64,
    pub can_toggle: bool,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_seek: bool,
    /// App ids of every session currently known to Windows.
    pub sources: Vec<String>,
    pub track_key: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    pub track_key: String,
    pub data_url: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MusicSnapshot {
    pub state: MusicState,
    pub artwork: Artwork,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum MusicCommand {
    Toggle,
    Next,
    Prev,
}

enum Cmd {
    Control(MusicCommand),
    Seek(i64),
    Select(Option<String>),
}

pub struct Music {
    tx: Mutex<Sender<Cmd>>,
    latest: Mutex<MusicSnapshot>,
}

pub fn init(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    app.manage(Music {
        tx: Mutex::new(tx),
        latest: Mutex::new(MusicSnapshot::default()),
    });
    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-music".into())
        .spawn(move || worker(app, rx))
        .expect("failed to spawn music thread");
}

fn worker(app: AppHandle, rx: Receiver<Cmd>) {
    let mut manager: Option<SessionManager> = None;
    let mut selected: Option<String> = None;
    let mut last = MusicState::default();
    let mut art_key = String::new();
    let mut art_found = false;
    let mut art_attempts = 0u8;

    loop {
        let cmd = match rx.recv_timeout(POLL) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };

        if manager.is_none() {
            manager = SessionManager::RequestAsync().and_then(|op| op.join()).ok();
        }
        let Some(mgr) = manager.as_ref() else {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };

        match cmd {
            Some(Cmd::Select(id)) => selected = id,
            Some(cmd) => {
                if let Some(s) = pick_session(mgr, &selected) {
                    if let Err(e) = run_command(&s, cmd) {
                        log::warn!("music command failed: {e}");
                    }
                    // Give the player a moment to apply it before re-reading.
                    std::thread::sleep(Duration::from_millis(150));
                }
            }
            None => {}
        }

        let (mut state, session) = read_state(mgr, &selected);
        if state.position_updated_at <= 0 {
            // Player did not report when the position was measured.
            let same = state.track_key == last.track_key && state.position_ms == last.position_ms;
            state.position_updated_at = if same { last.position_updated_at } else { now_ms() };
        }

        if state.track_key != art_key {
            art_key = state.track_key.clone();
            art_found = false;
            art_attempts = 0;
        }
        if !art_found && art_attempts < ARTWORK_ATTEMPTS {
            art_attempts += 1;
            let data_url = session.as_ref().and_then(|s| read_artwork(s).ok().flatten());
            art_found = data_url.is_some();
            if art_found || art_attempts == 1 {
                let artwork = Artwork { track_key: art_key.clone(), data_url };
                if let Ok(mut l) = app.state::<Music>().latest.lock() {
                    l.artwork = artwork.clone();
                }
                let _ = app.emit("music://artwork", &artwork);
            }
        }

        if state != last {
            last = state.clone();
            if let Ok(mut l) = app.state::<Music>().latest.lock() {
                l.state = state.clone();
            }
            let _ = app.emit("music://state", &state);
        }
    }
}

fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

fn pick_session(mgr: &SessionManager, selected: &Option<String>) -> Option<Session> {
    if let Some(id) = selected
        && let Ok(list) = mgr.GetSessions()
        && let Some(s) = list.into_iter().find(|s| &session_id(s) == id)
    {
        return Some(s);
    }
    mgr.GetCurrentSession().ok()
}

fn read_state(mgr: &SessionManager, selected: &Option<String>) -> (MusicState, Option<Session>) {
    let sources: Vec<String> = mgr
        .GetSessions()
        .map(|l| l.into_iter().map(|s| session_id(&s)).collect())
        .unwrap_or_default();
    let Some(s) = pick_session(mgr, selected) else {
        return (MusicState { sources, ..Default::default() }, None);
    };

    let mut st = MusicState {
        available: true,
        source_app_id: session_id(&s),
        sources,
        ..Default::default()
    };

    if let Ok(p) = s.TryGetMediaPropertiesAsync().and_then(|op| op.join()) {
        st.title = p.Title().map(|h| h.to_string()).unwrap_or_default();
        st.artist = p.Artist().map(|h| h.to_string()).unwrap_or_default();
        st.album = p.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
    }

    if let Ok(info) = s.GetPlaybackInfo() {
        st.playing = info.PlaybackStatus().map(|x| x == PlaybackStatus::Playing).unwrap_or(false);
        if let Ok(c) = info.Controls() {
            st.can_toggle = c.IsPlayPauseToggleEnabled().unwrap_or(false)
                || c.IsPlayEnabled().unwrap_or(false)
                || c.IsPauseEnabled().unwrap_or(false);
            st.can_next = c.IsNextEnabled().unwrap_or(false);
            st.can_prev = c.IsPreviousEnabled().unwrap_or(false);
            st.can_seek = c.IsPlaybackPositionEnabled().unwrap_or(false);
        }
    }

    if let Ok(t) = s.GetTimelineProperties() {
        let start = t.StartTime().map(|x| x.Duration).unwrap_or(0);
        let end = t.EndTime().map(|x| x.Duration).unwrap_or(0);
        let pos = t.Position().map(|x| x.Duration).unwrap_or(0);
        st.duration_ms = ((end - start) / 10_000).max(0);
        st.position_ms = ((pos - start) / 10_000).clamp(0, st.duration_ms.max(0));
        let updated = t.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);
        st.position_updated_at = if updated > 0 { (updated - WINRT_EPOCH_OFFSET) / 10_000 } else { 0 };
    }

    st.track_key = format!("{}|{}|{}", st.source_app_id, st.title, st.artist);
    (st, Some(s))
}

fn read_artwork(s: &Session) -> windows::core::Result<Option<String>> {
    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let Ok(thumb) = props.Thumbnail() else { return Ok(None) };
    let stream = thumb.OpenReadAsync()?.join()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(None);
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    let mime = stream
        .ContentType()
        .map(|h| h.to_string())
        .ok()
        .filter(|m| m.starts_with("image/"))
        .unwrap_or_else(|| "image/png".into());
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

fn run_command(s: &Session, cmd: Cmd) -> windows::core::Result<()> {
    match cmd {
        Cmd::Control(MusicCommand::Toggle) => {
            s.TryTogglePlayPauseAsync()?.join()?;
        }
        Cmd::Control(MusicCommand::Next) => {
            s.TrySkipNextAsync()?.join()?;
        }
        Cmd::Control(MusicCommand::Prev) => {
            s.TrySkipPreviousAsync()?.join()?;
        }
        Cmd::Seek(ms) => {
            let start = s.GetTimelineProperties()?.StartTime()?.Duration;
            s.TryChangePlaybackPositionAsync(start + ms.max(0) * 10_000)?.join()?;
        }
        Cmd::Select(_) => {}
    }
    Ok(())
}

fn send(music: &Music, cmd: Cmd) -> Result<(), String> {
    music
        .tx
        .lock()
        .map_err(|e| e.to_string())?
        .send(cmd)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn music_get(music: State<'_, Music>) -> Result<MusicSnapshot, String> {
    Ok(music.latest.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub fn music_control(music: State<'_, Music>, command: MusicCommand) -> Result<(), String> {
    send(&music, Cmd::Control(command))
}

#[tauri::command]
pub fn music_seek(music: State<'_, Music>, position_ms: i64) -> Result<(), String> {
    send(&music, Cmd::Seek(position_ms))
}

/// `None` follows whatever Windows considers the current session.
#[tauri::command]
pub fn music_select_source(music: State<'_, Music>, source_app_id: Option<String>) -> Result<(), String> {
    send(&music, Cmd::Select(source_app_id))
}
