use crate::island_window;
use serde::Deserialize;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, Wry};

pub const SETTINGS_LABEL: &str = "settings";

/// Menu labels come from the frontend so the tray follows the UI language.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub settings: String,
    pub hide: String,
    pub show: String,
    pub quit: String,
    pub tuck: String,
    pub untuck: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            settings: "Settings…".into(),
            hide: "Hide Island".into(),
            show: "Show Island".into(),
            quit: "Quit Duri".into(),
            tuck: "Tuck Away".into(),
            untuck: "Bring Back".into(),
        }
    }
}

struct Inner {
    labels: TrayLabels,
    hidden: bool,
}

pub struct Tray {
    settings: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    inner: Mutex<Inner>,
}

impl Tray {
    fn refresh(&self, inner: &Inner) {
        let _ = self.settings.set_text(&inner.labels.settings);
        let _ = self.quit.set_text(&inner.labels.quit);
        let _ = self.toggle.set_text(if inner.hidden { &inner.labels.show } else { &inner.labels.hide });
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let labels = TrayLabels::default();
    let settings = MenuItem::with_id(app, "settings", &labels.settings, true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", &labels.hide, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &labels.quit, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&settings, &toggle, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id("duri")
        .tooltip("Duri")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => open_settings(app),
            "toggle" => toggle_island(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open_settings(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    app.manage(Tray {
        settings,
        toggle,
        quit,
        inner: Mutex::new(Inner { labels, hidden: false }),
    });
    Ok(())
}

/// The island's right-click menu: the same commands as the tray.
#[tauri::command]
pub async fn island_context_menu(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    let labels = app.state::<Tray>().inner.lock().map_err(|e| e.to_string())?.labels.clone();
    let tucked = crate::settings::load(&app.state::<std::sync::Arc<crate::db::Db>>()).tucked;
    let item = |id: &str, text: &str| MenuItem::with_id(&app, id, text, true, None::<&str>);
    let menu = Menu::with_items(
        &app,
        &[
            &item("ctx-settings", &labels.settings).map_err(|e| e.to_string())?,
            &item("ctx-tuck", if tucked { &labels.untuck } else { &labels.tuck }).map_err(|e| e.to_string())?,
            &item("ctx-hide", &labels.hide).map_err(|e| e.to_string())?,
            &PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?,
            &item("ctx-quit", &labels.quit).map_err(|e| e.to_string())?,
        ],
    )
    .map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

/// App-wide menu handler; tray items are handled by the tray itself.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "ctx-settings" => open_settings(app),
        "ctx-hide" => toggle_island(app),
        "ctx-tuck" => crate::settings::toggle_tucked(app),
        "ctx-quit" => app.exit(0),
        _ => {}
    }
}

fn toggle_island(app: &AppHandle) {
    let tray = app.state::<Tray>();
    let Ok(mut inner) = tray.inner.lock() else { return };
    inner.hidden = !inner.hidden;
    island_window::set_hidden(app, inner.hidden);
    tray.refresh(&inner);
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        island_window::bring_to_front(&w);
        return;
    }
    let result = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("index.html".into()))
        .title("Duri Settings")
        .inner_size(680.0, 540.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .center()
        .build();
    match result {
        Ok(w) => island_window::bring_to_front(&w),
        Err(e) => log::error!("failed to open settings: {e}"),
    }
}

/// Async so the window is built off the main thread (sync commands that create
/// windows can deadlock on Windows).
#[tauri::command]
pub async fn open_settings_window(app: AppHandle) {
    open_settings(&app);
}

#[tauri::command]
pub fn tray_set_labels(tray: State<'_, Tray>, labels: TrayLabels) {
    if let Ok(mut inner) = tray.inner.lock() {
        inner.labels = labels;
        tray.refresh(&inner);
    }
}
