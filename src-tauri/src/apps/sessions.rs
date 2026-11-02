//! Sessions app: tracks Claude Code and Codex sessions.
//!
//! - **Claude Code** reports through hooks. Duri installs hook entries into
//!   `~/.claude/settings.json` that POST the hook payload to a local endpoint
//!   (`127.0.0.1:47321/duri/claude-hook`) with curl. Existing hooks are kept.
//! - **Codex** writes rollout files to `~/.codex/sessions/YYYY/MM/DD/*.jsonl`;
//!   a watcher tails the recent ones.
//!
//! The state logic lives in [`Book`] as plain functions so it can be tested
//! without either tool installed.

use crate::time::now_ms;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

pub const PORT: u16 = 47321;
/// Unique path segment; also how Duri recognises its own hook entries.
const HOOK_PATH: &str = "/duri/claude-hook";
const MAX_BODY: usize = 1 << 20;
/// Sessions with no news for this long are dropped.
const STALE_MS: i64 = 3 * 60 * 60 * 1000;
/// "Working" with no news for this long means the tool died or was closed.
const WORKING_TIMEOUT_MS: i64 = 15 * 60 * 1000;
/// Only Codex files touched this recently are read.
const CODEX_RECENT_MS: i64 = 6 * 60 * 60 * 1000;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Agent {
    Claude,
    Codex,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Open, nothing asked yet.
    Idle,
    Working,
    /// Needs the person: a permission prompt or an approval.
    Waiting,
    /// Finished its turn.
    Done,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub project: String,
    pub cwd: String,
    pub status: Status,
    pub detail: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    /// When `status` last changed.
    pub status_since: i64,
}

#[derive(Default)]
pub struct Book {
    sessions: HashMap<String, Session>,
}

pub fn project_name(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

fn snippet(text: &str, max: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max - 1).collect::<String>().trim_end())
    }
}

/// "Edit · main.rs", "Bash · cargo test".
fn describe_tool(name: &str, input: &Value) -> String {
    let arg = ["file_path", "path", "command", "pattern", "url", "description"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(|a| if a.contains(['/', '\\']) && !a.contains(' ') { project_name(a) } else { snippet(a, 48) });
    match arg {
        Some(a) if !a.is_empty() => format!("{name} · {a}"),
        _ => name.to_string(),
    }
}

impl Book {
    fn upsert(&mut self, id: &str, agent: Agent, cwd: &str, at: i64) -> &mut Session {
        let s = self.sessions.entry(id.to_string()).or_insert_with(|| Session {
            id: id.to_string(),
            agent,
            project: project_name(cwd),
            cwd: cwd.to_string(),
            status: Status::Idle,
            detail: None,
            started_at: at,
            updated_at: at,
            status_since: at,
        });
        if !cwd.is_empty() && s.cwd != cwd {
            s.cwd = cwd.to_string();
            s.project = project_name(cwd);
        }
        s
    }

    fn set(s: &mut Session, status: Status, detail: Option<String>, at: i64) {
        if s.status != status {
            s.status_since = at;
        }
        s.status = status;
        s.detail = detail;
        s.updated_at = at;
    }

    /// Applies one Claude Code hook payload. Returns whether anything changed.
    pub fn apply_claude(&mut self, ev: &Value, at: i64) -> bool {
        let Some(id) = ev.get("session_id").and_then(Value::as_str) else { return false };
        let event = ev.get("hook_event_name").and_then(Value::as_str).unwrap_or("");
        if event == "SessionEnd" {
            return self.sessions.remove(id).is_some();
        }
        let cwd = ev.get("cwd").and_then(Value::as_str).unwrap_or("");
        let s = self.upsert(id, Agent::Claude, cwd, at);
        let keep = (s.status, s.detail.clone());
        let (status, detail) = match event {
            "SessionStart" => (Status::Idle, None),
            "UserPromptSubmit" => (Status::Working, ev.get("prompt").and_then(Value::as_str).map(|p| snippet(p, 80))),
            "PreToolUse" => (
                Status::Working,
                ev.get("tool_name")
                    .and_then(Value::as_str)
                    .map(|t| describe_tool(t, ev.get("tool_input").unwrap_or(&Value::Null))),
            ),
            "PostToolUse" if s.status == Status::Waiting => (Status::Working, s.detail.clone()),
            "Notification" => {
                let msg = ev.get("message").and_then(Value::as_str).unwrap_or("");
                let kind = ev.get("notification_type").and_then(Value::as_str).unwrap_or("");
                let idle = kind == "idle_prompt" || msg.to_lowercase().contains("waiting for your input");
                if idle {
                    // Just a reminder that the turn is over.
                    (if keep.0 == Status::Working { Status::Done } else { keep.0 }, keep.1)
                } else {
                    (Status::Waiting, Some(snippet(msg, 80)).filter(|m| !m.is_empty()))
                }
            }
            "Stop" => (Status::Done, None),
            _ => keep,
        };
        Self::set(s, status, detail, at);
        true
    }

    /// Applies one line of a Codex rollout file. `key` identifies the file.
    pub fn apply_codex(&mut self, key: &str, line: &Value, at: i64) -> bool {
        let payload = line.get("payload").unwrap_or(&Value::Null);
        let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let id = format!("codex:{key}");
        match line.get("type").and_then(Value::as_str).unwrap_or("") {
            "session_meta" => {
                let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
                self.upsert(&id, Agent::Codex, cwd, at).updated_at = at;
                true
            }
            "event_msg" => {
                let s = self.upsert(&id, Agent::Codex, "", at);
                let (status, detail) = match kind {
                    "task_started" => (Status::Working, s.detail.clone()),
                    "user_message" => (
                        Status::Working,
                        payload.get("message").and_then(Value::as_str).map(|m| snippet(m, 80)),
                    ),
                    "exec_command_begin" => (
                        Status::Working,
                        payload
                            .get("command")
                            .map(|c| match c {
                                Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                                other => other.as_str().unwrap_or("").to_string(),
                            })
                            .map(|c| format!("shell · {}", snippet(&c, 48))),
                    ),
                    k if k.ends_with("approval_request") => (Status::Waiting, None),
                    "task_complete" | "turn_aborted" => (Status::Done, None),
                    _ => return false,
                };
                Self::set(s, status, detail, at);
                true
            }
            "response_item" if kind == "function_call" => {
                let s = self.upsert(&id, Agent::Codex, "", at);
                let name = payload.get("name").and_then(Value::as_str).map(str::to_string);
                Self::set(s, Status::Working, name, at);
                true
            }
            _ => false,
        }
    }

    pub fn dismiss(&mut self, id: &str) -> bool {
        self.sessions.remove(id).is_some()
    }

    /// Drops stale sessions and un-sticks "working" ones that went silent.
    pub fn prune(&mut self, now: i64) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| now - s.updated_at < STALE_MS);
        let mut changed = before != self.sessions.len();
        for s in self.sessions.values_mut() {
            if s.status == Status::Working && now - s.updated_at > WORKING_TIMEOUT_MS {
                s.status = Status::Idle;
                s.status_since = now;
                changed = true;
            }
        }
        changed
    }

    /// Needs-you first, then working, done, idle; newest first within each.
    pub fn list(&self) -> Vec<Session> {
        let rank = |s: Status| match s {
            Status::Waiting => 0,
            Status::Working => 1,
            Status::Done => 2,
            Status::Idle => 3,
        };
        let mut v: Vec<Session> = self.sessions.values().cloned().collect();
        v.sort_by(|a, b| rank(a.status).cmp(&rank(b.status)).then(b.updated_at.cmp(&a.updated_at)));
        v
    }
}

// ---------------------------------------------------------------------------
// Claude Code hook installation (pure JSON edits, tested below)

/// Events Duri listens to, with the matcher each needs.
const HOOK_EVENTS: &[(&str, Option<&str>)] = &[
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("PreToolUse", Some("*")),
    ("PostToolUse", Some("*")),
    ("Notification", None),
    ("Stop", None),
    ("SessionEnd", None),
];

pub fn hook_command() -> String {
    // The endpoint answers 204 with no body, so nothing reaches Claude's
    // context; `|| true` keeps Claude quiet when Duri isn't running.
    format!(
        "curl -s -m 2 -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{PORT}{HOOK_PATH} || true"
    )
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(HOOK_PATH))
}

pub fn hooks_installed(settings: &Value) -> bool {
    HOOK_EVENTS.iter().all(|(event, _)| {
        settings
            .pointer(&format!("/hooks/{event}"))
            .and_then(Value::as_array)
            .is_some_and(|groups| {
                groups.iter().any(|g| {
                    g.get("hooks").and_then(Value::as_array).is_some_and(|h| h.iter().any(is_ours))
                })
            })
    })
}

/// Removes every Duri hook, leaving the person's own hooks untouched.
pub fn uninstall_hooks(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else { return false };
    let mut changed = false;
    let mut emptied_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        let before = groups.len();
        groups.retain_mut(|g| {
            let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
            let n = list.len();
            list.retain(|h| !is_ours(h));
            // Drop a group only if it became empty because of us.
            !(list.is_empty() && n > 0)
        });
        let removed_hooks = groups.len() != before;
        changed |= removed_hooks;
        if groups.is_empty() && before > 0 {
            emptied_events.push(event.clone());
        }
    }
    for e in emptied_events {
        hooks.remove(&e);
    }
    changed
}

pub fn install_hooks(settings: &mut Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("~/.claude/settings.json is not a JSON object".into());
    }
    uninstall_hooks(settings);
    let root = settings.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().ok_or("\"hooks\" is not an object")?;
    for (event, matcher) in HOOK_EVENTS {
        let mut group = json!({ "hooks": [{ "type": "command", "command": hook_command(), "timeout": 5 }] });
        if let Some(m) = matcher {
            group.as_object_mut().unwrap().insert("matcher".into(), json!(m));
        }
        let list = hooks.entry(*event).or_insert_with(|| json!([]));
        list.as_array_mut().ok_or(format!("hooks.{event} is not an array"))?.push(group);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Runtime

pub struct Sessions {
    book: Mutex<Book>,
    server_running: Mutex<bool>,
    home: Option<PathBuf>,
}

fn emit(app: &AppHandle, book: &Book) {
    let _ = app.emit("sessions://changed", book.list());
}

fn with_book(app: &AppHandle, f: impl FnOnce(&mut Book) -> bool) {
    let state = app.state::<Sessions>();
    let Ok(mut book) = state.book.lock() else { return };
    if f(&mut book) {
        emit(app, &book);
    }
}

pub fn init(app: &AppHandle) {
    let home = app.path().home_dir().ok();
    app.manage(Sessions { book: Mutex::new(Book::default()), server_running: Mutex::new(false), home: home.clone() });
    spawn_server(app.clone());
    if let Some(home) = home {
        spawn_codex_watcher(app.clone(), home.join(".codex").join("sessions"));
    }
}

fn spawn_server(app: AppHandle) {
    std::thread::Builder::new()
        .name("duri-sessions-http".into())
        .spawn(move || {
            let server = match tiny_http::Server::http(("127.0.0.1", PORT)) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("sessions endpoint failed to start on port {PORT}: {e}");
                    return;
                }
            };
            if let Ok(mut r) = app.state::<Sessions>().server_running.lock() {
                *r = true;
            }
            for mut req in server.incoming_requests() {
                let ok = req.method() == &tiny_http::Method::Post && req.url() == HOOK_PATH;
                if ok {
                    let mut body = Vec::new();
                    let _ = req.as_reader().take(MAX_BODY as u64).read_to_end(&mut body);
                    if let Ok(ev) = serde_json::from_slice::<Value>(&body) {
                        with_book(&app, |b| b.apply_claude(&ev, now_ms()));
                    }
                }
                let _ = req.respond(tiny_http::Response::empty(if ok { 204 } else { 404 }));
            }
        })
        .expect("failed to spawn sessions server");
}

fn mtime_ms(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

/// The newest `n` entries of a directory, by name (Codex uses zero-padded dates).
fn newest_dirs(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    v.sort();
    v.into_iter().rev().take(n).collect()
}

fn recent_codex_files(root: &Path, now: i64) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for year in newest_dirs(root, 2) {
        for month in newest_dirs(&year, 2) {
            for day in newest_dirs(&month, 2) {
                if let Ok(rd) = fs::read_dir(&day) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.extension().is_some_and(|x| x == "jsonl") && now - mtime_ms(&p) < CODEX_RECENT_MS {
                            files.push(p);
                        }
                    }
                }
            }
        }
    }
    files
}

fn spawn_codex_watcher(app: AppHandle, root: PathBuf) {
    std::thread::Builder::new()
        .name("duri-sessions-codex".into())
        .spawn(move || {
            let mut offsets: HashMap<PathBuf, u64> = HashMap::new();
            loop {
                let now = now_ms();
                with_book(&app, |b| b.prune(now));
                if root.is_dir() {
                    for path in recent_codex_files(&root, now) {
                        let offset = offsets.get(&path).copied().unwrap_or(0);
                        let Ok(mut f) = fs::File::open(&path) else { continue };
                        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                        if len <= offset {
                            continue;
                        }
                        let mut buf = String::new();
                        if f.seek(SeekFrom::Start(offset)).is_err() || f.read_to_string(&mut buf).is_err() {
                            continue;
                        }
                        // Only consume complete lines; a partial last line is re-read next time.
                        let complete = buf.rfind('\n').map_or(0, |i| i + 1);
                        offsets.insert(path.clone(), offset + complete as u64);
                        // Lines from a file's first read are history: date them by the file.
                        let at = if offset == 0 { mtime_ms(&path) } else { now };
                        let key = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        with_book(&app, |b| {
                            let mut changed = false;
                            for line in buf[..complete].lines() {
                                if let Ok(v) = serde_json::from_str::<Value>(line) {
                                    changed |= b.apply_codex(&key, &v, at);
                                }
                            }
                            changed
                        });
                    }
                }
                std::thread::sleep(Duration::from_millis(1500));
            }
        })
        .expect("failed to spawn codex watcher");
}

fn claude_settings_path(state: &Sessions) -> Result<PathBuf, String> {
    state.home.as_ref().map(|h| h.join(".claude").join("settings.json")).ok_or("home folder not found".into())
}

fn read_settings(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({})),
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("could not parse {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn write_settings(path: &Path, value: &Value) -> Result<(), String> {
    if path.exists() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let backup = path.with_file_name(format!("settings.json.duri-backup-{stamp}"));
        fs::copy(path, &backup).map_err(|e| format!("backup failed: {e}"))?;
    } else if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n";
    let tmp = path.with_extension("json.duri-tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStatus {
    installed: bool,
    server_running: bool,
}

#[tauri::command]
pub fn sessions_get(state: State<'_, Sessions>) -> Result<Vec<Session>, String> {
    Ok(state.book.lock().map_err(|e| e.to_string())?.list())
}

#[tauri::command]
pub fn session_dismiss(app: AppHandle, id: String) {
    with_book(&app, |b| b.dismiss(&id));
}

#[tauri::command]
pub fn claude_hooks_status(state: State<'_, Sessions>) -> Result<HookStatus, String> {
    let settings = read_settings(&claude_settings_path(&state)?)?;
    Ok(HookStatus {
        installed: hooks_installed(&settings),
        server_running: state.server_running.lock().map(|r| *r).unwrap_or(false),
    })
}

#[tauri::command]
pub fn claude_hooks_install(state: State<'_, Sessions>) -> Result<(), String> {
    let path = claude_settings_path(&state)?;
    let mut settings = read_settings(&path)?;
    install_hooks(&mut settings)?;
    write_settings(&path, &settings)
}

#[tauri::command]
pub fn claude_hooks_uninstall(state: State<'_, Sessions>) -> Result<(), String> {
    let path = claude_settings_path(&state)?;
    let mut settings = read_settings(&path)?;
    if uninstall_hooks(&mut settings) {
        write_settings(&path, &settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(book: &mut Book, event: &str, extra: Value, at: i64) {
        let mut ev = json!({ "session_id": "s1", "cwd": "C:\\code\\duri", "hook_event_name": event });
        if let (Some(o), Some(e)) = (ev.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        assert!(book.apply_claude(&ev, at));
    }

    #[test]
    fn claude_session_lifecycle() {
        let mut b = Book::default();
        claude(&mut b, "SessionStart", json!({}), 1);
        let s = &b.list()[0];
        assert_eq!((s.status, s.project.as_str(), s.agent), (Status::Idle, "duri", Agent::Claude));

        claude(&mut b, "UserPromptSubmit", json!({ "prompt": "\n  fix the bug\nplease" }), 2);
        assert_eq!(b.list()[0].detail.as_deref(), Some("fix the bug"));

        claude(&mut b, "PreToolUse", json!({ "tool_name": "Edit", "tool_input": { "file_path": "C:\\code\\duri\\src\\main.rs" } }), 3);
        assert_eq!(b.list()[0].detail.as_deref(), Some("Edit · main.rs"));

        claude(&mut b, "Notification", json!({ "message": "Claude needs your permission to use Bash" }), 4);
        assert_eq!(b.list()[0].status, Status::Waiting);
        assert_eq!(b.list()[0].status_since, 4);

        claude(&mut b, "PostToolUse", json!({ "tool_name": "Bash" }), 5);
        assert_eq!(b.list()[0].status, Status::Working);

        claude(&mut b, "Stop", json!({}), 6);
        assert_eq!(b.list()[0].status, Status::Done);
        // The idle reminder does not turn a finished session into "needs you".
        claude(&mut b, "Notification", json!({ "message": "Claude is waiting for your input" }), 7);
        assert_eq!(b.list()[0].status, Status::Done);
        assert_eq!(b.list()[0].status_since, 6);

        claude(&mut b, "SessionEnd", json!({}), 8);
        assert!(b.list().is_empty());
    }

    #[test]
    fn ignores_payloads_without_session() {
        let mut b = Book::default();
        assert!(!b.apply_claude(&json!({ "hook_event_name": "Stop" }), 1));
    }

    #[test]
    fn codex_rollout_lines() {
        let mut b = Book::default();
        let lines = [
            json!({ "type": "session_meta", "payload": { "id": "x", "cwd": "/home/me/game" } }),
            json!({ "type": "event_msg", "payload": { "type": "user_message", "message": "add tests" } }),
            json!({ "type": "event_msg", "payload": { "type": "exec_command_begin", "command": ["cargo", "test"] } }),
        ];
        for l in &lines {
            assert!(b.apply_codex("rollout-1", l, 10));
        }
        let s = &b.list()[0];
        assert_eq!((s.agent, s.project.as_str(), s.status), (Agent::Codex, "game", Status::Working));
        assert_eq!(s.detail.as_deref(), Some("shell · cargo test"));

        b.apply_codex("rollout-1", &json!({ "type": "event_msg", "payload": { "type": "exec_approval_request" } }), 11);
        assert_eq!(b.list()[0].status, Status::Waiting);
        b.apply_codex("rollout-1", &json!({ "type": "event_msg", "payload": { "type": "task_complete" } }), 12);
        assert_eq!(b.list()[0].status, Status::Done);
        assert!(!b.apply_codex("rollout-1", &json!({ "type": "event_msg", "payload": { "type": "token_count" } }), 13));
    }

    #[test]
    fn prune_and_order() {
        let mut b = Book::default();
        b.apply_claude(&json!({ "session_id": "old", "hook_event_name": "UserPromptSubmit" }), 0);
        b.apply_claude(&json!({ "session_id": "a", "hook_event_name": "Stop" }), 100);
        b.apply_claude(&json!({ "session_id": "b", "hook_event_name": "Notification", "message": "needs permission" }), 50);
        assert_eq!(b.list().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "old", "a"]);
        assert!(b.prune(WORKING_TIMEOUT_MS + 1));
        assert_eq!(b.sessions["old"].status, Status::Idle);
        assert!(b.prune(STALE_MS + 200));
        assert!(b.list().is_empty());
    }

    #[test]
    fn hook_install_keeps_existing_hooks_and_is_reversible() {
        let original = json!({
            "theme": "dark",
            "hooks": {
                "PreToolUse": [{ "matcher": "Grep", "hooks": [{ "type": "command", "command": "mine.cmd" }] }]
            }
        });
        let mut s = original.clone();
        assert!(!hooks_installed(&s));
        install_hooks(&mut s).unwrap();
        assert!(hooks_installed(&s));
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(s["hooks"]["PreToolUse"][1]["matcher"], "*");

        // Installing twice doesn't duplicate.
        install_hooks(&mut s).unwrap();
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);

        assert!(uninstall_hooks(&mut s));
        assert_eq!(s, original);
        assert!(!uninstall_hooks(&mut s));
    }

    #[test]
    fn hook_install_on_empty_settings() {
        let mut s = json!({});
        install_hooks(&mut s).unwrap();
        assert!(hooks_installed(&s));
        uninstall_hooks(&mut s);
        assert_eq!(s, json!({ "hooks": {} }));
        assert!(install_hooks(&mut json!([])).is_err());
    }

    #[test]
    fn names_and_snippets() {
        assert_eq!(project_name("C:\\a\\b\\"), "b");
        assert_eq!(project_name("/x/y"), "y");
        assert_eq!(snippet("abcdef", 4), "abc…");
        assert_eq!(describe_tool("Bash", &json!({ "command": "cargo test --all" })), "Bash · cargo test --all");
        assert_eq!(describe_tool("TodoWrite", &json!({})), "TodoWrite");
    }
}
