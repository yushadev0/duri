//! Backend halves of Duri's apps. Each app module owns its state, background
//! work and `#[tauri::command]`s, and talks to the UI through events named
//! `<app>://<event>`. Register new commands in `lib.rs`.

pub mod clipboard;
pub mod music;
pub mod notes;
pub mod notifications;
pub mod sessions;
pub mod timers;
pub mod volume;

use crate::db::Db;
use std::path::Path;
use std::sync::Arc;
use tauri::AppHandle;

pub fn init(app: &AppHandle, db: Arc<Db>, data_dir: &Path) {
    let settings = crate::settings::load(&db);
    clipboard::init(app, db.clone(), data_dir);
    notes::init(app, db.clone());
    timers::init(app, db);
    music::init(app);
    notifications::init(app);
    sessions::init(app);
    volume::init(app, settings.replace_volume_flyout);
}
