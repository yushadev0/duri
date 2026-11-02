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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            disabled_apps: Vec::new(),
            island_style: "floating".into(),
            pill_size: "default".into(),
        }
    }
}

#[tauri::command]
pub fn settings_get(db: State<'_, Arc<Db>>) -> Settings {
    db.get_json(NS, KEY).unwrap_or_default()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, db: State<'_, Arc<Db>>, settings: Settings) -> Result<(), String> {
    db.set_json(NS, KEY, &settings)?;
    app.emit("settings://changed", &settings).map_err(|e| e.to_string())
}
