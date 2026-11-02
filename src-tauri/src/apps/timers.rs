//! Timer and Stopwatch apps.
//!
//! State is stored as absolute wall-clock timestamps, not counters, so it
//! survives restarts without drift: a running timer stores when it ends, a
//! running stopwatch stores when it (re)started. The frontend derives the
//! numbers it shows from the same timestamps.

use crate::db::Db;
use crate::time::now_ms;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const NS: &str = "timers";
const KEY: &str = "snapshot";
pub const MAX_TIMERS: usize = 8;
pub const MIN_DURATION_MS: i64 = 1_000;
pub const MAX_DURATION_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TimerState {
    Running { ends_at: i64 },
    Paused { remaining_ms: i64 },
    Finished { finished_at: i64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub id: String,
    pub label: Option<String>,
    pub duration_ms: i64,
    pub created_at: i64,
    #[serde(flatten)]
    pub state: TimerState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Stopwatch {
    /// Set while running: the moment the current run segment started.
    pub running_since: Option<i64>,
    /// Time banked from previous run segments.
    pub accumulated_ms: i64,
    /// Total elapsed time at each lap press (cumulative, not split).
    pub laps: Vec<i64>,
}

impl Stopwatch {
    pub fn elapsed(&self, now: i64) -> i64 {
        self.accumulated_ms + self.running_since.map_or(0, |s| (now - s).max(0))
    }
    pub fn start(&mut self, now: i64) {
        if self.running_since.is_none() {
            self.running_since = Some(now);
        }
    }
    pub fn pause(&mut self, now: i64) {
        if let Some(s) = self.running_since.take() {
            self.accumulated_ms += (now - s).max(0);
        }
    }
    pub fn lap(&mut self, now: i64) {
        if self.running_since.is_some() {
            self.laps.push(self.elapsed(now));
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Snapshot {
    pub timers: Vec<Timer>,
    pub stopwatch: Stopwatch,
}

impl Snapshot {
    fn find(&mut self, id: &str) -> Option<&mut Timer> {
        self.timers.iter_mut().find(|t| t.id == id)
    }

    pub fn start_timer(&mut self, duration_ms: i64, label: Option<String>, now: i64) -> Result<String, String> {
        if self.timers.len() >= MAX_TIMERS {
            return Err(format!("at most {MAX_TIMERS} timers"));
        }
        let duration_ms = duration_ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS);
        let id = uuid::Uuid::new_v4().to_string();
        self.timers.push(Timer {
            id: id.clone(),
            label: label.filter(|l| !l.trim().is_empty()),
            duration_ms,
            created_at: now,
            state: TimerState::Running { ends_at: now + duration_ms },
        });
        Ok(id)
    }

    pub fn pause_timer(&mut self, id: &str, now: i64) {
        if let Some(t) = self.find(id)
            && let TimerState::Running { ends_at } = t.state
        {
            t.state = TimerState::Paused { remaining_ms: (ends_at - now).max(0) };
        }
    }

    pub fn resume_timer(&mut self, id: &str, now: i64) {
        if let Some(t) = self.find(id)
            && let TimerState::Paused { remaining_ms } = t.state
        {
            t.state = TimerState::Running { ends_at: now + remaining_ms };
        }
    }

    /// Runs a timer again from its full duration (used for "Repeat").
    pub fn restart_timer(&mut self, id: &str, now: i64) {
        if let Some(t) = self.find(id) {
            t.state = TimerState::Running { ends_at: now + t.duration_ms };
        }
    }

    /// Removes a timer whatever its state (cancel and dismiss).
    pub fn remove_timer(&mut self, id: &str) {
        self.timers.retain(|t| t.id != id);
    }

    /// Marks running timers whose end has passed as finished and returns them.
    pub fn tick(&mut self, now: i64) -> Vec<Timer> {
        let mut done = Vec::new();
        for t in &mut self.timers {
            if let TimerState::Running { ends_at } = t.state
                && ends_at <= now
            {
                t.state = TimerState::Finished { finished_at: ends_at };
                done.push(t.clone());
            }
        }
        done
    }
}

pub struct Timers {
    db: Arc<Db>,
    snap: Mutex<Snapshot>,
}

fn commit(app: &AppHandle, timers: &Timers, snap: &Snapshot) {
    if let Err(e) = timers.db.set_json(NS, KEY, snap) {
        log::error!("saving timers failed: {e}");
    }
    let _ = app.emit("timers://changed", snap);
}

fn mutate<R>(app: &AppHandle, timers: &Timers, f: impl FnOnce(&mut Snapshot) -> R) -> Result<R, String> {
    let mut snap = timers.snap.lock().map_err(|e| e.to_string())?;
    let out = f(&mut snap);
    commit(app, timers, &snap);
    Ok(out)
}

pub fn init(app: &AppHandle, db: Arc<Db>) {
    let mut snap: Snapshot = db.get_json(NS, KEY).unwrap_or_default();
    // Timers that ran out while Duri was closed are shown as finished.
    snap.tick(now_ms());
    app.manage(Timers { db, snap: Mutex::new(snap) });

    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-timers".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(200));
            let timers = app.state::<Timers>();
            let Ok(mut snap) = timers.snap.lock() else { continue };
            let done = snap.tick(now_ms());
            if !done.is_empty() {
                commit(&app, &timers, &snap);
                drop(snap);
                for t in done {
                    let _ = app.emit("timers://finished", &t);
                }
            }
        })
        .expect("failed to spawn timer thread");
}

#[tauri::command]
pub fn timers_get(timers: State<'_, Timers>) -> Result<Snapshot, String> {
    Ok(timers.snap.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub fn timer_start(app: AppHandle, timers: State<'_, Timers>, duration_ms: i64, label: Option<String>) -> Result<String, String> {
    mutate(&app, &timers, |s| s.start_timer(duration_ms, label, now_ms()))?
}

#[tauri::command]
pub fn timer_pause(app: AppHandle, timers: State<'_, Timers>, id: String) -> Result<(), String> {
    mutate(&app, &timers, |s| s.pause_timer(&id, now_ms()))
}

#[tauri::command]
pub fn timer_resume(app: AppHandle, timers: State<'_, Timers>, id: String) -> Result<(), String> {
    mutate(&app, &timers, |s| s.resume_timer(&id, now_ms()))
}

#[tauri::command]
pub fn timer_restart(app: AppHandle, timers: State<'_, Timers>, id: String) -> Result<(), String> {
    mutate(&app, &timers, |s| s.restart_timer(&id, now_ms()))
}

#[tauri::command]
pub fn timer_remove(app: AppHandle, timers: State<'_, Timers>, id: String) -> Result<(), String> {
    mutate(&app, &timers, |s| s.remove_timer(&id))
}

#[tauri::command]
pub fn stopwatch_start(app: AppHandle, timers: State<'_, Timers>) -> Result<(), String> {
    mutate(&app, &timers, |s| s.stopwatch.start(now_ms()))
}

#[tauri::command]
pub fn stopwatch_pause(app: AppHandle, timers: State<'_, Timers>) -> Result<(), String> {
    mutate(&app, &timers, |s| s.stopwatch.pause(now_ms()))
}

#[tauri::command]
pub fn stopwatch_lap(app: AppHandle, timers: State<'_, Timers>) -> Result<(), String> {
    mutate(&app, &timers, |s| s.stopwatch.lap(now_ms()))
}

#[tauri::command]
pub fn stopwatch_reset(app: AppHandle, timers: State<'_, Timers>) -> Result<(), String> {
    mutate(&app, &timers, |s| s.stopwatch.reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_lifecycle() {
        let mut s = Snapshot::default();
        let id = s.start_timer(60_000, None, 1_000).unwrap();
        assert_eq!(s.timers[0].state, TimerState::Running { ends_at: 61_000 });

        s.pause_timer(&id, 11_000);
        assert_eq!(s.timers[0].state, TimerState::Paused { remaining_ms: 50_000 });
        // A paused timer never finishes.
        assert!(s.tick(1_000_000).is_empty());

        s.resume_timer(&id, 100_000);
        assert_eq!(s.timers[0].state, TimerState::Running { ends_at: 150_000 });
        assert!(s.tick(149_999).is_empty());
        let done = s.tick(150_000);
        assert_eq!(done.len(), 1);
        assert_eq!(s.timers[0].state, TimerState::Finished { finished_at: 150_000 });
        // Finishing is reported once.
        assert!(s.tick(200_000).is_empty());

        s.restart_timer(&id, 200_000);
        assert_eq!(s.timers[0].state, TimerState::Running { ends_at: 260_000 });
        s.remove_timer(&id);
        assert!(s.timers.is_empty());
    }

    #[test]
    fn timer_duration_is_clamped_and_count_limited() {
        let mut s = Snapshot::default();
        s.start_timer(5, Some("  ".into()), 0).unwrap();
        assert_eq!(s.timers[0].duration_ms, MIN_DURATION_MS);
        assert_eq!(s.timers[0].label, None);
        for _ in 1..MAX_TIMERS {
            s.start_timer(1_000, None, 0).unwrap();
        }
        assert!(s.start_timer(1_000, None, 0).is_err());
    }

    #[test]
    fn stopwatch_accumulates_segments_and_laps() {
        let mut w = Stopwatch::default();
        w.lap(0); // ignored while stopped
        w.start(1_000);
        w.lap(3_000);
        w.pause(5_000);
        assert_eq!(w.elapsed(99_000), 4_000);
        w.start(10_000);
        w.start(11_000); // no-op while running
        assert_eq!(w.elapsed(12_000), 6_000);
        w.lap(12_000);
        assert_eq!(w.laps, vec![2_000, 6_000]);
        w.reset();
        assert_eq!(w, Stopwatch::default());
    }

    #[test]
    fn snapshot_json_shape() {
        let t = Timer {
            id: "a".into(),
            label: None,
            duration_ms: 1,
            created_at: 0,
            state: TimerState::Paused { remaining_ms: 5 },
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["state"], "paused");
        assert_eq!(v["remainingMs"], 5);
        assert_eq!(v["durationMs"], 1);
    }
}
