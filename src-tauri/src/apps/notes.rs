//! Notes app: quick plain-text notes, saved as you type.
//!
//! The first line of a note is its title. Notes that end up empty are
//! deleted instead of kept as blank entries.

use crate::db::Db;
use crate::time::now_ms;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub body: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn row_to_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        body: r.get(1)?,
        pinned: r.get::<_, i64>(2)? != 0,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
    })
}

const COLUMNS: &str = "id, body, pinned, created_at, updated_at";

pub fn create(conn: &Connection, body: &str, now: i64) -> rusqlite::Result<Note> {
    let note = Note { id: uuid::Uuid::new_v4().to_string(), body: body.to_string(), pinned: false, created_at: now, updated_at: now };
    conn.execute(
        "INSERT INTO notes (id, body, pinned, created_at, updated_at) VALUES (?1, ?2, 0, ?3, ?3)",
        params![note.id, note.body, now],
    )?;
    Ok(note)
}

/// Saves a new body; an empty body deletes the note. Returns the saved note.
pub fn update(conn: &Connection, id: &str, body: &str, now: i64) -> rusqlite::Result<Option<Note>> {
    if body.trim().is_empty() {
        delete(conn, id)?;
        return Ok(None);
    }
    conn.execute("UPDATE notes SET body = ?1, updated_at = ?2 WHERE id = ?3 AND body != ?1", params![body, now, id])?;
    get(conn, id)
}

pub fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<Note>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM notes WHERE id = ?1"), [id], row_to_note).optional()
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
    Ok(())
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE notes SET pinned = ?1 WHERE id = ?2", params![pinned as i64, id])?;
    Ok(())
}

/// Pinned first, then most recently edited.
pub fn list(conn: &Connection, query: Option<&str>) -> rusqlite::Result<Vec<Note>> {
    let like = query.filter(|q| !q.trim().is_empty()).map(|q| {
        let escaped = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        format!("%{escaped}%")
    });
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM notes WHERE (?1 IS NULL OR body LIKE ?1 ESCAPE '\\')
         ORDER BY pinned DESC, updated_at DESC"
    ))?;
    stmt.query_map(params![like], row_to_note)?.collect()
}

pub struct Notes {
    db: Arc<Db>,
}

pub fn init(app: &AppHandle, db: Arc<Db>) {
    app.manage(Notes { db });
}

fn changed(app: &AppHandle) {
    let _ = app.emit("notes://changed", ());
}

#[tauri::command]
pub fn notes_list(state: State<'_, Notes>, query: Option<String>) -> Result<Vec<Note>, String> {
    state.db.with_conn(|c| list(c, query.as_deref()))
}

#[tauri::command]
pub fn note_create(app: AppHandle, state: State<'_, Notes>, body: String) -> Result<Note, String> {
    let note = state.db.with_conn(|c| create(c, &body, now_ms()))?;
    changed(&app);
    Ok(note)
}

#[tauri::command]
pub fn note_update(app: AppHandle, state: State<'_, Notes>, id: String, body: String) -> Result<Option<Note>, String> {
    let note = state.db.with_conn(|c| update(c, &id, &body, now_ms()))?;
    changed(&app);
    Ok(note)
}

#[tauri::command]
pub fn note_delete(app: AppHandle, state: State<'_, Notes>, id: String) -> Result<(), String> {
    state.db.with_conn(|c| delete(c, &id))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn note_set_pinned(app: AppHandle, state: State<'_, Notes>, id: String, pinned: bool) -> Result<(), String> {
    state.db.with_conn(|c| set_pinned(c, &id, pinned))?;
    changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_update_and_order() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            let a = create(c, "Groceries\nmilk", 1)?;
            let b = create(c, "Ideas", 2)?;
            assert_eq!(list(c, None)?.iter().map(|n| n.id.clone()).collect::<Vec<_>>(), [b.id.clone(), a.id.clone()]);

            // Editing moves a note up; saving the same text doesn't.
            update(c, &a.id, "Groceries\nmilk, eggs", 3)?;
            assert_eq!(list(c, None)?[0].id, a.id);
            update(c, &b.id, "Ideas", 9)?;
            assert_eq!(get(c, &b.id)?.unwrap().updated_at, 2);

            // Pinned notes come first.
            set_pinned(c, &b.id, true)?;
            assert_eq!(list(c, None)?[0].id, b.id);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn emptying_a_note_deletes_it() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            let n = create(c, "x", 1)?;
            assert_eq!(update(c, &n.id, "  \n ", 2)?, None);
            assert!(list(c, None)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_is_case_insensitive_and_escaped() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            create(c, "Meeting notes", 1)?;
            create(c, "50% off", 2)?;
            assert_eq!(list(c, Some("meeting"))?.len(), 1);
            assert_eq!(list(c, Some("%"))?.len(), 1);
            assert_eq!(list(c, Some("nothing"))?.len(), 0);
            Ok(())
        })
        .unwrap();
    }
}
