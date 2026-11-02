//! Local SQLite storage. Lives at `%APPDATA%\Duri\duri.db`.
//!
//! Schema changes are append-only entries in [`MIGRATIONS`]; the index of the
//! last applied entry is kept in `PRAGMA user_version`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Append new migrations to the end. Never edit or reorder existing entries.
const MIGRATIONS: &[&str] = &[
    // 1: generic namespaced key/value store (settings, small app state)
    "CREATE TABLE kv (
        ns    TEXT NOT NULL,
        key   TEXT NOT NULL,
        value TEXT NOT NULL,
        PRIMARY KEY (ns, key)
    );",
    // 2: clipboard history
    "CREATE TABLE clips (
        id         TEXT PRIMARY KEY,
        kind       TEXT NOT NULL,
        text       TEXT,
        files      TEXT,
        image_path TEXT,
        thumb      TEXT,
        width      INTEGER,
        height     INTEGER,
        hash       TEXT NOT NULL UNIQUE,
        pinned     INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        used_at    INTEGER NOT NULL
    );
    CREATE INDEX clips_order ON clips (pinned DESC, used_at DESC);",
];

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        migrate(&conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    /// Runs `f` with the connection, for apps that own real tables.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<R>) -> Result<R, String> {
        let conn = self.0.lock().map_err(|e| e.to_string())?;
        f(&conn).map_err(|e| e.to_string())
    }

    pub fn get_json<T: DeserializeOwned>(&self, ns: &str, key: &str) -> Option<T> {
        let conn = self.0.lock().ok()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM kv WHERE ns = ?1 AND key = ?2",
                params![ns, key],
                |r| r.get(0),
            )
            .optional()
            .ok()?;
        raw.and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn set_json<T: Serialize>(&self, ns: &str, key: &str, value: &T) -> Result<(), String> {
        let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let conn = self.0.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO kv (ns, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT (ns, key) DO UPDATE SET value = excluded.value",
            params![ns, key, json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let applied: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        let v: i64 = db
            .0
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
    }

    #[test]
    fn kv_roundtrip_and_overwrite() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_json::<Vec<i32>>("a", "b"), None);
        db.set_json("a", "b", &vec![1, 2]).unwrap();
        db.set_json("a", "b", &vec![3]).unwrap();
        assert_eq!(db.get_json::<Vec<i32>>("a", "b"), Some(vec![3]));
    }
}
