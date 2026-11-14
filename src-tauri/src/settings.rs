use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

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
    /// "system" (follow Windows' time format), "12h" or "24h".
    pub clock_format: String,
    /// "off", "low" or "high": how much the desktop shows through the island.
    pub transparency: String,
    /// Hide the island while a fullscreen app, game or video is in front.
    pub hide_in_fullscreen: bool,
    /// Look for new versions on GitHub Releases at launch and twice a day.
    pub auto_update_check: bool,
    /// The island is tucked into a small handle at the top edge.
    pub tucked: bool,
    /// Like an auto-hiding taskbar: invisible at rest, back when the pointer
    /// reaches the top edge of the screen.
    pub auto_hide: bool,
    /// Global shortcuts as accelerators ("Ctrl+Alt+Space"); empty = off.
    pub shortcut_island: String,
    pub shortcut_note: String,
    pub shortcut_clipboard: String,
    /// Privacy app: show apps using the camera.
    pub privacy_camera: bool,
    /// Privacy app: show apps using the microphone.
    pub privacy_microphone: bool,
    /// Replace the Windows tray clock's text. Loads a helper into
    /// explorer.exe, so it is off until the user opts in.
    pub taskbar_clock: bool,
    /// The clock's lines, see `taskbar_clock::template`. Blank ones are
    /// left out.
    pub taskbar_clock_line1: String,
    pub taskbar_clock_line2: String,
    pub taskbar_clock_line3: String,
    /// "left", "center" or "right".
    pub taskbar_clock_align: String,
    /// The city the weather tokens are for: its name as shown, and where it
    /// is. Nothing is fetched until one is picked.
    pub taskbar_clock_city: String,
    pub taskbar_clock_latitude: Option<f64>,
    pub taskbar_clock_longitude: Option<f64>,
    pub taskbar_clock_fahrenheit: bool,
    /// Music › Idle Mode look: "auto" (Network, or Lyrics when the system
    /// asks for reduced motion), "network", "reactive", "lyrics" or "cover".
    pub music_idle_style: String,
    /// Music: the line being sung shows in the middle of the pill, for songs
    /// with timed lyrics (asks LRCLIB for every song played while on).
    pub music_pill_lyrics: bool,
    /// Music: the time beside the lyrics in the pill.
    pub music_pill_clock: bool,
    /// Music › Idle Mode: a clock at the top of the screen.
    pub music_idle_clock: bool,
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
            clock_format: "system".into(),
            transparency: "off".into(),
            hide_in_fullscreen: true,
            auto_update_check: true,
            tucked: false,
            auto_hide: false,
            // Only the island has a default. Other apps own all sorts of
            // global shortcuts (Ctrl+Alt+Shift+N opens Laravel Herd's site
            // wizard), and Ctrl+Alt+letter is AltGr+letter on many layouts,
            // so people pick the rest themselves in Settings.
            shortcut_island: "Ctrl+Alt+Space".into(),
            shortcut_note: String::new(),
            shortcut_clipboard: String::new(),
            privacy_camera: true,
            privacy_microphone: true,
            taskbar_clock: false,
            taskbar_clock_line1: "{time}".into(),
            taskbar_clock_line2: "CPU {cpu}%  RAM {ram}%".into(),
            taskbar_clock_line3: String::new(),
            taskbar_clock_align: "right".into(),
            taskbar_clock_city: String::new(),
            taskbar_clock_latitude: None,
            taskbar_clock_longitude: None,
            taskbar_clock_fahrenheit: false,
            music_idle_style: "auto".into(),
            music_pill_lyrics: false,
            music_pill_clock: false,
            music_idle_clock: true,
        }
    }
}

impl Settings {
    /// Volume keys are taken over only while the Volume app is on: otherwise
    /// they'd be swallowed with nothing shown instead.
    pub fn takes_volume_keys(&self) -> bool {
        self.replace_volume_flyout && !self.disabled_apps.iter().any(|a| a == "volume")
    }
}

pub fn load(db: &Db) -> Settings {
    db.get_json(NS, KEY).unwrap_or_default()
}

#[tauri::command]
pub fn settings_get(db: State<'_, Arc<Db>>) -> Settings {
    load(&db)
}

/// Saves the settings, applies the backend-side ones and tells every window.
pub fn save(app: &AppHandle, db: &Db, settings: &Settings) -> Result<(), String> {
    let previous = load(db);
    db.set_json(NS, KEY, settings)?;
    crate::apps::volume::set_replace_flyout(settings.takes_volume_keys());
    crate::island_window::set_hide_in_fullscreen(settings.hide_in_fullscreen);
    // Re-registering every time would briefly drop the shortcuts.
    if crate::shortcuts::wanted(&previous) != crate::shortcuts::wanted(settings) {
        crate::shortcuts::apply(app, settings);
    }
    crate::taskbar_clock::apply(app, db, &previous, settings);
    app.emit("settings://changed", settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_set(app: AppHandle, db: State<'_, Arc<Db>>, settings: Settings) -> Result<(), String> {
    save(&app, &db, &settings)
}

/// Tucks the island away or brings it back (island right-click menu).
pub fn toggle_tucked(app: &AppHandle) {
    let db = app.state::<Arc<Db>>();
    let mut settings = load(&db);
    settings.tucked = !settings.tucked;
    if let Err(e) = save(app, &db, &settings) {
        log::warn!("could not save settings: {e}");
    }
}
