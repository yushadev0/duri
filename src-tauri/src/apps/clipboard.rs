//! Clipboard app: a history of copied text, images, and files.
//!
//! A clipboard-format listener wakes a thread on every change. Content that
//! asks to be kept out of history (password managers set
//! `ExcludeClipboardContentFromMonitorProcessing` /
//! `CanIncludeInClipboardHistory = 0`) is skipped. Images are stored as PNG
//! files under `%APPDATA%\Duri\clips` with a small thumbnail in the database.
//! Identical content is not duplicated: copying it again moves it to the top.

use crate::db::Db;
use crate::island_window;
use crate::time::now_ms;
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// Unpinned items kept; older ones are deleted.
pub const KEEP: usize = 300;
const MAX_TEXT_BYTES: usize = 1 << 20;
const THUMB_PX: u32 = 160;
const LIST_LIMIT: i64 = 150;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    /// "text", "image" or "files"
    pub kind: String,
    pub text: Option<String>,
    pub files: Vec<String>,
    pub thumb: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pinned: bool,
    pub created_at: i64,
    pub used_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Files(Vec<String>),
    /// Already-saved image: path to the PNG plus thumbnail data URL.
    Image { path: String, thumb: String, width: u32, height: u32 },
}

fn hash_of(kind: &str, bytes: &[u8]) -> String {
    let mut h = DefaultHasher::new();
    kind.hash(&mut h);
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ---------------------------------------------------------------------------
// Storage (plain functions over a connection; tested below)

const COLUMNS: &str = "id, kind, text, files, thumb, width, height, pinned, created_at, used_at";

fn row_to_clip(r: &rusqlite::Row) -> rusqlite::Result<Clip> {
    let files: Option<String> = r.get(3)?;
    Ok(Clip {
        id: r.get(0)?,
        kind: r.get(1)?,
        text: r.get(2)?,
        files: files.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default(),
        thumb: r.get(4)?,
        width: r.get(5)?,
        height: r.get(6)?,
        pinned: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
        used_at: r.get(9)?,
    })
}

/// Inserts new content or, if it is already known, moves it to the top.
/// Returns the item id and any image files that trimming made obsolete.
pub fn upsert(conn: &Connection, hash: &str, content: &Content, now: i64) -> rusqlite::Result<(String, Vec<String>)> {
    if let Some(id) = conn
        .query_row("SELECT id FROM clips WHERE hash = ?1", [hash], |r| r.get::<_, String>(0))
        .optional()?
    {
        conn.execute("UPDATE clips SET used_at = ?1 WHERE id = ?2", params![now, id])?;
        return Ok((id, Vec::new()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (kind, text, files, image_path, thumb, w, h) = match content {
        Content::Text(t) => ("text", Some(t.clone()), None, None, None, None, None),
        Content::Files(f) => ("files", None, serde_json::to_string(f).ok(), None, None, None, None),
        Content::Image { path, thumb, width, height } => {
            ("image", None, None, Some(path.clone()), Some(thumb.clone()), Some(*width), Some(*height))
        }
    };
    conn.execute(
        "INSERT INTO clips (id, kind, text, files, image_path, thumb, width, height, hash, pinned, created_at, used_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?10)",
        params![id, kind, text, files, image_path, thumb, w, h, hash, now],
    )?;
    let removed = trim(conn, KEEP)?;
    Ok((id, removed))
}

/// Deletes unpinned items beyond `keep`, returning their image files.
pub fn trim(conn: &Connection, keep: usize) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id, image_path FROM clips WHERE pinned = 0 ORDER BY used_at DESC LIMIT -1 OFFSET ?1",
    )?;
    let old: Vec<(String, Option<String>)> =
        stmt.query_map([keep as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (id, _) in &old {
        conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
    }
    Ok(old.into_iter().filter_map(|(_, p)| p).collect())
}

pub fn list(conn: &Connection, query: Option<&str>, kind: Option<&str>) -> rusqlite::Result<Vec<Clip>> {
    let like = query.filter(|q| !q.trim().is_empty()).map(|q| {
        let escaped = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });
    let sql = format!(
        "SELECT {COLUMNS} FROM clips
         WHERE (?1 IS NULL OR text LIKE ?1 ESCAPE '\\' OR files LIKE ?1 ESCAPE '\\')
           AND (?2 IS NULL OR kind = ?2)
         ORDER BY pinned DESC, used_at DESC LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_map(params![like, kind, LIST_LIMIT], row_to_clip)?.collect()
}

fn get_with_path(conn: &Connection, id: &str) -> rusqlite::Result<Option<(Clip, Option<String>)>> {
    conn.query_row(
        &format!("SELECT {COLUMNS}, image_path FROM clips WHERE id = ?1"),
        [id],
        |r| Ok((row_to_clip(r)?, r.get::<_, Option<String>>(10)?)),
    )
    .optional()
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE clips SET pinned = ?1 WHERE id = ?2", params![pinned as i64, id])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    let path = get_with_path(conn, id)?.and_then(|(_, p)| p);
    conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
    Ok(path)
}

pub fn clear_unpinned(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    trim(conn, 0)
}

// ---------------------------------------------------------------------------
// System clipboard

mod system {
    use super::*;
    use clipboard_win::{formats, raw, Clipboard, Getter, Setter};

    pub enum Raw {
        Text(String),
        Files(Vec<String>),
        /// BMP file bytes.
        Bitmap(Vec<u8>),
    }

    fn excluded() -> bool {
        let present = |name: &str| raw::register_format(name).is_some_and(|f| raw::is_format_avail(f.get()));
        if present("ExcludeClipboardContentFromMonitorProcessing") {
            return true;
        }
        // CanIncludeInClipboardHistory = 0 means "don't keep me".
        if let Some(f) = raw::register_format("CanIncludeInClipboardHistory")
            && raw::is_format_avail(f.get())
        {
            let mut v: Vec<u8> = Vec::new();
            let _ = formats::RawData(f.get()).read_clipboard(&mut v);
            return v.iter().all(|b| *b == 0);
        }
        false
    }

    pub fn read() -> Option<Raw> {
        let _guard = Clipboard::new_attempts(10).ok()?;
        if excluded() {
            return None;
        }
        if raw::is_format_avail(formats::CF_HDROP) {
            let files: Vec<String> = clipboard_win::get(formats::FileList).unwrap_or_default();
            if !files.is_empty() {
                return Some(Raw::Files(files));
            }
        }
        if raw::is_format_avail(formats::CF_UNICODETEXT) {
            let text: String = clipboard_win::get(formats::Unicode).unwrap_or_default();
            if !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES {
                return Some(Raw::Text(text));
            }
        }
        if raw::is_format_avail(formats::CF_BITMAP) || raw::is_format_avail(formats::CF_DIB) {
            let bmp: Vec<u8> = clipboard_win::get(formats::Bitmap).unwrap_or_default();
            if !bmp.is_empty() {
                return Some(Raw::Bitmap(bmp));
            }
        }
        None
    }

    pub fn write(raw_content: &Raw) -> Result<(), String> {
        let _guard = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        match raw_content {
            Raw::Text(t) => formats::Unicode.write_clipboard(t),
            Raw::Files(f) => formats::FileList.write_clipboard(f.as_slice()),
            Raw::Bitmap(b) => formats::Bitmap.write_clipboard(b),
        }
        .map_err(|e| e.to_string())
    }

    pub fn wait_for_change() -> Option<clipboard_win::monitor::Monitor> {
        clipboard_win::monitor::Monitor::new().ok()
    }

}

fn png_data_url(img: &image::DynamicImage) -> Option<String> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.get_ref())))
}

/// Turns raw clipboard data into storable content plus its hash.
fn prepare(raw: system::Raw, dir: &Path) -> Option<(String, Content)> {
    match raw {
        system::Raw::Text(t) => Some((hash_of("text", t.as_bytes()), Content::Text(t))),
        system::Raw::Files(f) => Some((hash_of("files", f.join("\n").as_bytes()), Content::Files(f))),
        system::Raw::Bitmap(bmp) => {
            let img = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
            let hash = hash_of("image", img.as_bytes());
            let path = dir.join(format!("{hash}.png"));
            if !path.exists() {
                img.save_with_format(&path, image::ImageFormat::Png).ok()?;
            }
            let thumb = png_data_url(&img.thumbnail(THUMB_PX, THUMB_PX))?;
            Some((
                hash,
                Content::Image { path: path.to_string_lossy().to_string(), thumb, width: img.width(), height: img.height() },
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime

pub struct ClipboardApp {
    db: Arc<Db>,
}

fn changed(app: &AppHandle) {
    let _ = app.emit("clipboard://changed", ());
}

fn remove_files(paths: &[String]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

pub fn init(app: &AppHandle, db: Arc<Db>, data_dir: &Path) {
    let dir = data_dir.join("clips");
    let _ = std::fs::create_dir_all(&dir);
    app.manage(ClipboardApp { db: db.clone() });

    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-clipboard".into())
        .spawn(move || {
            let Some(mut monitor) = system::wait_for_change() else {
                log::error!("clipboard listener could not start");
                return;
            };
            loop {
                match monitor.recv() {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                }
                // Let the owner finish writing every format.
                std::thread::sleep(Duration::from_millis(60));
                let Some((hash, content)) = system::read().and_then(|raw| prepare(raw, &dir)) else { continue };
                match db.with_conn(|c| upsert(c, &hash, &content, now_ms())) {
                    Ok((_, obsolete)) => {
                        remove_files(&obsolete);
                        changed(&app);
                    }
                    Err(e) => log::error!("saving clip failed: {e}"),
                }
            }
        })
        .expect("failed to spawn clipboard thread");
}

/// Puts a stored item back on the system clipboard.
fn copy_back(state: &ClipboardApp, id: &str) -> Result<(), String> {
    let (clip, path) = state.db.with_conn(|c| get_with_path(c, id))?.ok_or("item not found")?;
    let raw = match clip.kind.as_str() {
        "text" => system::Raw::Text(clip.text.unwrap_or_default()),
        "files" => system::Raw::Files(clip.files),
        _ => {
            let path = PathBuf::from(path.ok_or("image file missing")?);
            let img = image::open(&path).map_err(|e| e.to_string())?.to_rgb8();
            let mut bmp = Cursor::new(Vec::new());
            img.write_to(&mut bmp, image::ImageFormat::Bmp).map_err(|e| e.to_string())?;
            system::Raw::Bitmap(bmp.into_inner())
        }
    };
    system::write(&raw)
}

#[cfg(windows)]
fn send_ctrl_v() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL, VK_V,
    };
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[tauri::command]
pub fn clips_list(state: State<'_, ClipboardApp>, query: Option<String>, kind: Option<String>) -> Result<Vec<Clip>, String> {
    state.db.with_conn(|c| list(c, query.as_deref(), kind.as_deref()))
}

#[tauri::command]
pub fn clip_copy(state: State<'_, ClipboardApp>, id: String) -> Result<(), String> {
    copy_back(&state, &id)
}

/// Copies the item and pastes it into the app the person was using.
#[tauri::command]
pub async fn clip_paste(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        copy_back(&app.state::<ClipboardApp>(), &id)?;
        island_window::release_focus(&app);
        std::thread::sleep(Duration::from_millis(90));
        #[cfg(windows)]
        send_ctrl_v();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn clip_set_pinned(app: AppHandle, state: State<'_, ClipboardApp>, id: String, pinned: bool) -> Result<(), String> {
    state.db.with_conn(|c| set_pinned(c, &id, pinned))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn clip_delete(app: AppHandle, state: State<'_, ClipboardApp>, id: String) -> Result<(), String> {
    let path = state.db.with_conn(|c| delete(c, &id))?;
    remove_files(&path.into_iter().collect::<Vec<_>>());
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn clips_clear(app: AppHandle, state: State<'_, ClipboardApp>) -> Result<(), String> {
    let paths = state.db.with_conn(clear_unpinned)?;
    remove_files(&paths);
    changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn text(t: &str) -> (String, Content) {
        (hash_of("text", t.as_bytes()), Content::Text(t.into()))
    }

    #[test]
    fn dedupes_and_orders_by_use() {
        let db = db();
        db.with_conn(|c| {
            let (h1, c1) = text("hello");
            let (h2, c2) = text("world");
            let (id1, _) = upsert(c, &h1, &c1, 1)?;
            upsert(c, &h2, &c2, 2)?;
            assert_eq!(list(c, None, None)?.iter().map(|x| x.text.clone().unwrap()).collect::<Vec<_>>(), ["world", "hello"]);
            // Copying "hello" again moves it up instead of duplicating it.
            let (again, _) = upsert(c, &h1, &c1, 3)?;
            assert_eq!(again, id1);
            let all = list(c, None, None)?;
            assert_eq!(all.len(), 2);
            assert_eq!(all[0].text.as_deref(), Some("hello"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_filter_and_escaping() {
        let db = db();
        db.with_conn(|c| {
            for (i, t) in ["100% done", "file_name", "other"].iter().enumerate() {
                let (h, ct) = text(t);
                upsert(c, &h, &ct, i as i64)?;
            }
            let files = Content::Files(vec!["C:\\docs\\report.pdf".into()]);
            upsert(c, &hash_of("files", b"r"), &files, 9)?;
            assert_eq!(list(c, Some("%"), None)?.len(), 1);
            assert_eq!(list(c, Some("_"), None)?.len(), 1);
            assert_eq!(list(c, Some("report"), None)?[0].files, ["C:\\docs\\report.pdf"]);
            assert_eq!(list(c, Some("  "), None)?.len(), 4);
            assert_eq!(list(c, None, Some("files"))?.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pinned_items_survive_trim_and_clear() {
        let db = db();
        db.with_conn(|c| {
            let (h, ct) = text("keep me");
            let (pinned, _) = upsert(c, &h, &ct, 0)?;
            set_pinned(c, &pinned, true)?;
            for i in 0..KEEP + 5 {
                let (h, ct) = text(&format!("item {i}"));
                upsert(c, &h, &ct, 10 + i as i64)?;
            }
            let all = list(c, None, None)?;
            assert_eq!(all[0].id, pinned);
            let total: i64 = c.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
            assert_eq!(total as usize, KEEP + 1);

            clear_unpinned(c)?;
            assert_eq!(list(c, None, None)?.len(), 1);
            delete(c, &pinned)?;
            assert!(list(c, None, None)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn trim_reports_image_files() {
        let db = db();
        db.with_conn(|c| {
            let img = Content::Image { path: "a.png".into(), thumb: "t".into(), width: 1, height: 1 };
            upsert(c, "img", &img, 0)?;
            assert_eq!(clear_unpinned(c)?, ["a.png"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn image_prepare_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let img = image::DynamicImage::new_rgb8(400, 200);
        let mut bmp = Cursor::new(Vec::new());
        img.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let (hash, content) = prepare(system::Raw::Bitmap(bmp.into_inner()), dir.path()).unwrap();
        let Content::Image { path, thumb, width, height } = content else { panic!("not an image") };
        assert_eq!((width, height), (400, 200));
        assert!(Path::new(&path).exists());
        assert!(thumb.starts_with("data:image/png;base64,"));
        assert_eq!(hash.len(), 16);
    }
}
