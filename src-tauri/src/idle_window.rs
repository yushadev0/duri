//! Music › Idle Mode: a borderless fullscreen window on the island's monitor
//! that shows the song (word network, lyrics or cover). It opens only from
//! the button in the Music app and closes with Esc or its close button.
//!
//! While it is in front it counts as a fullscreen app, so the island steps
//! aside (`fullscreen_window_in_front`) unless the person turned that off.

use crate::island_window;
use std::sync::atomic::{AtomicIsize, Ordering};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "idle";

/// HWND of the idle window, 0 while closed.
static HWND: AtomicIsize = AtomicIsize::new(0);

pub fn hwnd() -> isize {
    HWND.load(Ordering::SeqCst)
}

pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        island_window::bring_to_front(&w);
        return;
    }
    crate::webview_runtime::pin_to_running();
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Duri")
        .decorations(false)
        .inner_size(960.0, 600.0)
        // Black before the page paints, so opening never flashes white.
        .background_color(tauri::window::Color(0, 0, 0, 255));
    // Fullscreen fills the monitor the window starts on: the island's one.
    if let Ok(Some(m)) = app.primary_monitor() {
        let p = m.position().to_logical::<f64>(m.scale_factor());
        builder = builder.position(p.x + 40.0, p.y + 40.0);
    }
    match builder.fullscreen(true).build() {
        Ok(w) => {
            if let Ok(h) = w.hwnd() {
                HWND.store(h.0 as isize, Ordering::SeqCst);
            }
            w.on_window_event(|e| {
                if matches!(e, WindowEvent::Destroyed) {
                    HWND.store(0, Ordering::SeqCst);
                    crate::apps::meter::stop();
                }
            });
            island_window::bring_to_front(&w);
        }
        Err(e) => log::error!("failed to open idle mode: {e}"),
    }
}

/// Async so the window is built off the main thread (see `open_settings_window`).
#[tauri::command]
pub async fn music_idle_open(app: AppHandle) {
    open(&app);
}

#[tauri::command]
pub fn music_idle_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}
