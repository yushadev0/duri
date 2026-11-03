//! Notifications app: mirrors Windows toast notifications into the island.
//!
//! One worker thread owns the WinRT `UserNotificationListener`. Reading the
//! toast list costs real CPU (every toast in Action Center comes back), so it
//! is read only when something changed. `NotificationChanged` is documented
//! for packaged apps only and fails here, and folder change notifications
//! don't fire for the database file Windows keeps open. So once a second the
//! worker looks at the size and write time of Windows' notification database
//! log (`wpndatabase.db-wal`), one cheap file query, and reads the toasts only
//! when those moved, plus a safety read every 30 s. Notifications are
//! kept in memory only (the last `KEEP`), never written to disk. Duri's own
//! toasts (a finished timer) are skipped: the island already shows them.

use crate::time::now_ms;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// How often the notification database's write stamp is looked at.
const TICK: Duration = Duration::from_millis(1000);
/// The toast list is read at least this often, changed stamp or not.
const SAFETY_CHECK: Duration = Duration::from_secs(30);
/// Without a database to watch, the toast list is read this often.
const FALLBACK_POLL: Duration = Duration::from_millis(2500);
/// The privacy setting is re-checked this often.
const ACCESS_CHECK: Duration = Duration::from_secs(10);
/// How many recent notifications the island keeps.
const KEEP: usize = 30;
const ICON_PX: f32 = 64.0;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// Windows' notification id.
    pub id: u32,
    /// App display name, e.g. "Discord".
    pub app: String,
    pub app_id: String,
    pub title: String,
    pub body: String,
    /// When Windows received it (Unix ms).
    pub at: i64,
    /// The app's logo as a data URL, when Windows has one.
    pub icon: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    #[default]
    Unknown,
    Allowed,
    /// Turned off in Settings › Privacy › Notifications.
    Denied,
    /// The listener isn't available on this Windows.
    Unavailable,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub access: Access,
    /// Newest first.
    pub items: Vec<Notice>,
}

/// What changed between two looks at the toast list.
#[derive(Debug, PartialEq, Default)]
pub struct Diff {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}

/// Compares the ids seen last time with the ids present now.
pub fn diff(seen: &HashSet<u32>, now: &[u32]) -> Diff {
    let current: HashSet<u32> = now.iter().copied().collect();
    let mut added: Vec<u32> = now.iter().copied().filter(|id| !seen.contains(id)).collect();
    added.dedup();
    let mut removed: Vec<u32> = seen.iter().copied().filter(|id| !current.contains(id)).collect();
    removed.sort_unstable();
    Diff { added, removed }
}

/// Whether a toast comes from Duri itself.
pub fn is_own(app_id: &str, app: &str) -> bool {
    app_id.to_ascii_lowercase().contains("app.duri") || app.eq_ignore_ascii_case("duri")
}

/// Windows FILETIME-style ticks (100 ns since 1601) to Unix ms.
fn ticks_to_unix_ms(ticks: i64) -> i64 {
    (ticks - 116_444_736_000_000_000) / 10_000
}

enum Cmd {
    Dismiss(u32),
    Clear,
    /// Windows says the toast list changed.
    Refresh,
}

pub struct Notifications {
    tx: Mutex<Sender<Cmd>>,
    latest: Mutex<Snapshot>,
}

pub fn init(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    app.manage(Notifications { tx: Mutex::new(tx.clone()), latest: Mutex::new(Snapshot::default()) });
    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-notifications".into())
        .spawn(move || worker(app, rx, tx))
        .expect("failed to spawn notifications thread");
}

fn publish(app: &AppHandle, snapshot: &Snapshot) {
    if let Ok(mut l) = app.state::<Notifications>().latest.lock() {
        *l = snapshot.clone();
    }
    let _ = app.emit("notifications://changed", snapshot);
}

#[cfg(windows)]
mod toast {
    use super::{ticks_to_unix_ms, Notice, ICON_PX};
    use base64::Engine;
    use std::collections::HashMap;
    use windows::Foundation::Size;
    use windows::Storage::Streams::DataReader;
    use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
    use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds, UserNotification};

    pub fn listener() -> windows::core::Result<UserNotificationListener> {
        UserNotificationListener::Current()
    }

    /// Size and last write of Windows' notification database log, or None
    /// when it can't be found. Changes whenever a toast arrives or goes.
    pub fn database_stamp() -> Option<(u64, std::time::SystemTime)> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let wal = std::path::Path::new(&local).join(r"Microsoft\Windows\Notifications\wpndatabase.db-wal");
        // `metadata` opens the file, so it sees the live size and time even
        // while Windows keeps it open (the folder entry lags behind).
        let m = std::fs::metadata(wal).ok()?;
        Some((m.len(), m.modified().ok()?))
    }

    /// Asks Windows to call `on_change` when toasts arrive or go. Documented
    /// for packaged apps only; unpackaged builds get an error.
    pub fn subscribe(l: &UserNotificationListener, on_change: impl Fn() + Send + Sync + 'static) -> bool {
        use windows::Foundation::TypedEventHandler;
        l.NotificationChanged(&TypedEventHandler::new(move |_, _| {
            on_change();
            Ok(())
        }))
        .is_ok()
    }

    pub fn allowed(l: &UserNotificationListener) -> Option<bool> {
        match l.GetAccessStatus().ok()? {
            UserNotificationListenerAccessStatus::Allowed => Some(true),
            UserNotificationListenerAccessStatus::Denied => Some(false),
            _ => {
                // Unspecified: ask once; Windows answers from the privacy setting.
                let status = l.RequestAccessAsync().ok()?.join().ok()?;
                Some(status == UserNotificationListenerAccessStatus::Allowed)
            }
        }
    }

    pub fn current(l: &UserNotificationListener) -> windows::core::Result<Vec<UserNotification>> {
        let list = l.GetNotificationsAsync(NotificationKinds::Toast)?.join()?;
        Ok(list.into_iter().collect())
    }

    pub fn id(n: &UserNotification) -> u32 {
        n.Id().unwrap_or(0)
    }

    fn icon(n: &UserNotification) -> windows::core::Result<Option<String>> {
        let logo = n.AppInfo()?.DisplayInfo()?.GetLogo(Size { Width: ICON_PX, Height: ICON_PX })?;
        let stream = logo.OpenReadAsync()?.join()?;
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
        Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf))))
    }

    /// Reads one toast. The app's logo is looked up once per app.
    pub fn read(n: &UserNotification, icons: &mut HashMap<String, Option<String>>) -> Option<Notice> {
        let info = n.AppInfo().ok();
        let app_id = info.as_ref().and_then(|i| i.AppUserModelId().ok()).map(|h| h.to_string()).unwrap_or_default();
        let app = info
            .as_ref()
            .and_then(|i| i.DisplayInfo().ok())
            .and_then(|d| d.DisplayName().ok())
            .map(|h| h.to_string())
            .unwrap_or_default();
        let texts: Vec<String> = n
            .Notification()
            .and_then(|t| t.Visual())
            .and_then(|v| v.GetBinding(&KnownNotificationBindings::ToastGeneric()?))
            .and_then(|b| b.GetTextElements())
            .map(|els| els.into_iter().filter_map(|e| e.Text().ok()).map(|h| h.to_string()).collect())
            .unwrap_or_default();
        let mut texts = texts.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        let title = texts.next().unwrap_or_default();
        let body = texts.collect::<Vec<_>>().join("\n");
        if title.is_empty() && body.is_empty() {
            return None;
        }
        let icon = icons.entry(app_id.clone()).or_insert_with(|| icon(n).ok().flatten()).clone();
        let at = n.CreationTime().map(|t| ticks_to_unix_ms(t.UniversalTime)).unwrap_or_else(|_| super::now_ms());
        Some(Notice { id: id(n), app, app_id, title, body, at, icon })
    }
}

#[cfg(windows)]
fn worker(app: AppHandle, rx: Receiver<Cmd>, tx: Sender<Cmd>) {
    let Ok(listener) = toast::listener() else {
        publish(&app, &Snapshot { access: Access::Unavailable, items: Vec::new() });
        return;
    };
    // Packaged builds get the real event; it just adds a Refresh.
    let subscribed = toast::subscribe(&listener, move || {
        let _ = tx.send(Cmd::Refresh);
    });
    let mut stamp = toast::database_stamp();
    let watching = subscribed || stamp.is_some();
    let mut last_read = std::time::Instant::now();
    let mut snapshot = Snapshot::default();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut icons: HashMap<String, Option<String>> = HashMap::new();
    let mut first = true;
    let mut access = Access::Unknown;
    let mut access_checked: Option<std::time::Instant> = None;
    loop {
        // The first pass runs at once; later ones wait for a change (or the
        // safety check) and take every queued change in one go.
        let cmd = if first { Ok(Cmd::Refresh) } else { rx.recv_timeout(if watching { TICK } else { FALLBACK_POLL }) };
        let cmd = match cmd {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        match cmd {
            Some(Cmd::Dismiss(id)) => {
                let _ = listener.RemoveNotification(id);
                snapshot.items.retain(|n| n.id != id);
                publish(&app, &snapshot);
            }
            Some(Cmd::Clear) => {
                for n in &snapshot.items {
                    let _ = listener.RemoveNotification(n.id);
                }
                snapshot.items.clear();
                publish(&app, &snapshot);
            }
            Some(Cmd::Refresh) => {}
            None if watching && !subscribed => {
                // Nothing to do unless the database moved (or it's been a while).
                let now = toast::database_stamp();
                let moved = now != stamp;
                stamp = now;
                if !moved && last_read.elapsed() < SAFETY_CHECK {
                    continue;
                }
            }
            None if subscribed && last_read.elapsed() < SAFETY_CHECK => continue,
            None => {}
        }
        // Several changes often arrive together (a toast, then its update).
        while let Ok(more) = rx.try_recv() {
            if let Cmd::Dismiss(id) = more {
                let _ = listener.RemoveNotification(id);
                snapshot.items.retain(|n| n.id != id);
            }
        }

        if access_checked.is_none_or(|at| at.elapsed() >= ACCESS_CHECK) {
            access = match toast::allowed(&listener) {
                Some(true) => Access::Allowed,
                Some(false) => Access::Denied,
                None => Access::Unavailable,
            };
            access_checked = Some(std::time::Instant::now());
        }
        if access != Access::Allowed {
            if snapshot.access != access {
                snapshot = Snapshot { access, items: Vec::new() };
                publish(&app, &snapshot);
            }
            first = false;
            continue;
        }

        last_read = std::time::Instant::now();
        let Ok(current) = toast::current(&listener) else { continue };
        let ids: Vec<u32> = current.iter().map(toast::id).collect();
        let change = diff(&seen, &ids);
        seen = ids.iter().copied().collect();
        // What's already in Action Center when Duri starts isn't news.
        let mut fresh = Vec::new();
        if !first {
            for n in current.iter().filter(|n| change.added.contains(&toast::id(n))) {
                if let Some(notice) = toast::read(n, &mut icons).filter(|x| !is_own(&x.app_id, &x.app)) {
                    fresh.push(notice);
                }
            }
        }
        let changed_access = snapshot.access != Access::Allowed;
        let before = snapshot.items.len();
        snapshot.access = Access::Allowed;
        // Dismissed in Action Center: drop it here too.
        snapshot.items.retain(|n| !change.removed.contains(&n.id));
        let removed_any = snapshot.items.len() != before;
        for notice in fresh.iter().rev() {
            snapshot.items.insert(0, notice.clone());
        }
        snapshot.items.truncate(KEEP);
        if first || changed_access || removed_any || !fresh.is_empty() {
            publish(&app, &snapshot);
        }
        for notice in &fresh {
            let _ = app.emit("notifications://new", notice);
        }
        first = false;
    }
}

#[cfg(not(windows))]
fn worker(app: AppHandle, _rx: Receiver<Cmd>, _tx: Sender<Cmd>) {
    publish(&app, &Snapshot { access: Access::Unavailable, items: Vec::new() });
}

#[tauri::command]
pub fn notifications_get(state: State<'_, Notifications>) -> Snapshot {
    state.latest.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Removes one notification from the island and from Action Center.
#[tauri::command]
pub fn notification_dismiss(state: State<'_, Notifications>, id: u32) {
    if let Ok(tx) = state.tx.lock() {
        let _ = tx.send(Cmd::Dismiss(id));
    }
}

/// Removes every notification the island shows (also from Action Center).
#[tauri::command]
pub fn notifications_clear(state: State<'_, Notifications>) {
    if let Ok(tx) = state.tx.lock() {
        let _ = tx.send(Cmd::Clear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_new_and_gone() {
        let seen: HashSet<u32> = [1, 2, 3].into_iter().collect();
        let d = diff(&seen, &[2, 3, 4, 5]);
        assert_eq!(d.added, vec![4, 5]);
        assert_eq!(d.removed, vec![1]);
        assert_eq!(diff(&seen, &[1, 2, 3]), Diff::default());
    }

    #[test]
    fn own_toasts_are_recognised() {
        assert!(is_own("app.duri", "Duri"));
        assert!(is_own("C:\\Program Files\\x\\app.duri.exe", ""));
        assert!(is_own("", "duri"));
        assert!(!is_own("Discord", "Discord"));
    }

    #[test]
    fn filetime_to_unix() {
        // 2026-11-03T00:00:00Z
        assert_eq!(ticks_to_unix_ms(134381376000000000), 1793664000000);
    }
}
