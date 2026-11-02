use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

const NS: &str = "settings";
const KEY: &str = "app";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system", "en" or "tr"
    pub language: String,
    /// App ids the user switched off in Settings › Apps.
    pub disabled_apps: Vec<String>,
    /// "floating" (below the screen edge) or "attached" (hangs from it).
    pub island_style: String,
    /// "compact", "default" or "large": scale of the idle pill.
    pub pill_size: String,
    /// Volume keys show Duri's indicator instead of the Windows popup.
    pub replace_volume_flyout: bool,
    /// What the empty idle pill shows: "none", "clock", "stats" or "both".
    pub idle_info: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            disabled_apps: Vec::new(),
            island_style: "floating".into(),
            pill_size: "default".into(),
            replace_volume_flyout: false,
            idle_info: "clock".into(),
        }
    }
}

pub fn load(db: &Db) -> Settings {
    db.get_json(NS, KEY).unwrap_or_default()
}

#[tauri::command]
pub fn settings_get(db: State<'_, Arc<Db>>) -> Settings {
    load(&db)
}

#[tauri::command]
pub fn settings_set(app: AppHandle, db: State<'_, Arc<Db>>, settings: Settings) -> Result<(), String> {
    db.set_json(NS, KEY, &settings)?;
    crate::apps::volume::set_replace_flyout(settings.replace_volume_flyout);
    app.emit("settings://changed", &settings).map_err(|e| e.to_string())
}
