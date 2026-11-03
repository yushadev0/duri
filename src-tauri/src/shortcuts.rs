//! Global keyboard shortcuts (Settings › General › Keyboard Shortcuts).
//!
//! Each shortcut is an accelerator string such as "Ctrl+Alt+Space"; an empty
//! string turns it off. Pressing one sends `island://shortcut` with the action
//! ("island", "note" or "clipboard") to the island, which opens and takes the
//! keyboard. A shortcut another app already owns can't be registered; the
//! failures are reported to Settings.

use crate::settings::Settings;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Default)]
pub struct Shortcuts {
    /// Registered shortcut id → action.
    actions: Mutex<HashMap<u32, &'static str>>,
    /// Actions whose shortcut couldn't be registered.
    failed: Mutex<Vec<String>>,
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let action = app.state::<Shortcuts>().actions.lock().ok().and_then(|a| a.get(&shortcut.id()).copied());
            if let Some(action) = action {
                let _ = app.emit_to(crate::island_window::LABEL, "island://shortcut", action);
            }
        })
        .build()
}

/// The shortcuts in `settings`, by action.
pub fn wanted(settings: &Settings) -> [(&'static str, &str); 3] {
    [
        ("island", settings.shortcut_island.as_str()),
        ("note", settings.shortcut_note.as_str()),
        ("clipboard", settings.shortcut_clipboard.as_str()),
    ]
}

/// Registers the shortcuts from the settings, replacing the previous ones.
pub fn apply(app: &AppHandle, settings: &Settings) {
    let state = app.state::<Shortcuts>();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut actions = HashMap::new();
    let mut failed = Vec::new();
    for (action, accel) in wanted(settings) {
        if accel.trim().is_empty() {
            continue;
        }
        match accel.parse::<Shortcut>() {
            Ok(shortcut) => {
                let id = shortcut.id();
                match gs.register(shortcut) {
                    Ok(()) => {
                        actions.insert(id, action);
                    }
                    Err(e) => {
                        log::warn!("shortcut {accel} for {action} is taken: {e}");
                        failed.push(action.to_string());
                    }
                }
            }
            Err(e) => {
                log::warn!("shortcut {accel} for {action} is invalid: {e}");
                failed.push(action.to_string());
            }
        }
    }
    if let Ok(mut a) = state.actions.lock() {
        *a = actions;
    }
    if let Ok(mut f) = state.failed.lock() {
        *f = failed.clone();
    }
    let _ = app.emit("shortcuts://failed", failed);
}

/// Actions whose shortcut couldn't be registered (taken or invalid).
#[tauri::command]
pub fn shortcuts_failed(state: State<'_, Shortcuts>) -> Vec<String> {
    state.failed.lock().map(|f| f.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::Shortcut;

    #[test]
    fn default_shortcuts_parse() {
        for accel in ["Ctrl+Alt+Space", "Ctrl+Alt+Shift+KeyN", "Ctrl+Alt+Shift+KeyV", "Super+Shift+Digit1"] {
            assert!(accel.parse::<Shortcut>().is_ok(), "{accel}");
        }
        assert!("Ctrl+Nope".parse::<Shortcut>().is_err());
    }
}
