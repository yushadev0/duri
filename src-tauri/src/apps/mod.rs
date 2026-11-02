//! Backend halves of Duri's apps. Each app module owns its state, background
//! work and `#[tauri::command]`s, and talks to the UI through events named
//! `<app>://<event>`. Register new commands in `lib.rs`.

pub mod clipboard;
pub mod music;
pub mod sessions;
pub mod timers;

use crate::db::Db;
use std::path::Path;
use std::sync::Arc;
use tauri::AppHandle;

pub fn init(app: &AppHandle, db: Arc<Db>, data_dir: &Path) {
    clipboard::init(app, db.clone(), data_dir);
    timers::init(app, db);
    music::init(app);
    sessions::init(app);
}
