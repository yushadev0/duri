//! The island overlay window: a transparent, always-on-top, non-activating
//! window pinned to the top-center of the primary monitor.
//!
//! The window is much larger than the visible island, so the transparent part
//! must not swallow clicks. The frontend reports the island's visible
//! rectangles (`island_set_hit_rects`); a background thread watches the cursor
//! and turns cursor events on only while it is over one of them. The same
//! thread tells the frontend about hover via the `island://pointer` event,
//! because the webview never sees `mouseleave` once events are ignored.

use serde::Deserialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "island";
/// Logical size of the overlay window. Must fit the largest expanded app plus
/// its shadow; keep in sync with `WINDOW_WIDTH`/`WINDOW_HEIGHT` in
/// `src/island/geometry.ts`.
pub const WIDTH: f64 = 640.0;
pub const HEIGHT: f64 = 320.0;

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Default)]
struct Inner {
    rects: Vec<Rect>,
    inside: bool,
    hidden: bool,
    /// The island currently has keyboard focus (someone is typing in it).
    focused: bool,
    /// Last foreground window that isn't Duri's (raw HWND): where focus
    /// returns and where pasting goes.
    last_external: isize,
}

#[derive(Default)]
pub struct IslandState(Mutex<Inner>);

impl IslandState {
    pub fn set_hidden(&self, hidden: bool) {
        if let Ok(mut s) = self.0.lock() {
            s.hidden = hidden;
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Duri")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
    position(&win)?;
    apply_non_activating_style(&win);
    win.set_ignore_cursor_events(true)?;
    show_without_activating(&win);
    Ok(win)
}

/// Shows the island and hands the foreground straight back to whatever was
/// in front, so showing it never steals focus.
pub fn show_without_activating(win: &WebviewWindow) {
    #[cfg(windows)]
    let before = fg::current();
    let _ = win.show();
    #[cfg(windows)]
    if before != 0 && !fg::is_ours(before) {
        if let Ok(mut s) = win.app_handle().state::<IslandState>().0.lock() {
            s.last_external = before;
        }
        fg::activate(before);
    }
}

/// Centers the window horizontally at the very top of the primary monitor.
pub fn position(win: &WebviewWindow) -> tauri::Result<()> {
    if let Some(m) = win.primary_monitor()? {
        let scale = m.scale_factor();
        let _ = win.set_size(tauri::LogicalSize::new(WIDTH, HEIGHT));
        let w = (WIDTH * scale).round() as i32;
        let x = m.position().x + (m.size().width as i32 - w) / 2;
        win.set_position(PhysicalPosition::new(x, m.position().y))?;
    }
    Ok(())
}

/// WS_EX_NOACTIVATE keeps focus in whatever the user is typing into when they
/// click the island; WS_EX_TOOLWINDOW keeps it out of Alt+Tab.
#[cfg(windows)]
fn apply_non_activating_style(win: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    if let Ok(h) = win.hwnd() {
        let hwnd = HWND(h.0 as _);
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize,
            );
        }
    }
}

#[cfg(not(windows))]
fn apply_non_activating_style(_win: &WebviewWindow) {}

/// Foreground-window helpers. Windows are passed around as raw `isize`
/// handles so they can live in shared state.
#[cfg(windows)]
mod fg {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};

    pub fn current() -> isize {
        unsafe { GetForegroundWindow().0 as isize }
    }

    /// Whether the window belongs to Duri (island or settings).
    pub fn is_ours(h: isize) -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(HWND(h as _), Some(&mut pid)) };
        pid == std::process::id()
    }

    pub fn activate(h: isize) {
        if h != 0 {
            unsafe {
                let _ = SetForegroundWindow(HWND(h as _));
            }
        }
    }

    /// Brings one of Duri's own windows to the front even when Windows'
    /// foreground lock would refuse it (the click that asked for focus went
    /// to WebView2's process, not ours). Briefly sharing the current
    /// foreground thread's input state is the standard way around that.
    pub fn force_activate(h: isize) {
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::WindowsAndMessaging::BringWindowToTop;
        unsafe {
            let target = HWND(h as _);
            let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let me = GetCurrentThreadId();
            let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
            let _ = BringWindowToTop(target);
            let _ = SetForegroundWindow(target);
            if attached {
                let _ = AttachThreadInput(me, fg_thread, false);
            }
        }
    }
}

/// Brings one of Duri's normal windows (e.g. Settings) to the front, even
/// when the request came from a click inside the island.
pub fn bring_to_front(win: &WebviewWindow) {
    let _ = win.show();
    let _ = win.unminimize();
    #[cfg(windows)]
    if let Ok(h) = win.hwnd() {
        fg::force_activate(h.0 as isize);
    }
    let _ = win.set_focus();
}

#[cfg(windows)]
fn island_hwnd(app: &AppHandle) -> Option<isize> {
    app.get_webview_window(LABEL).and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize)
}

/// Lets the island take keyboard focus (for text fields), or gives focus back
/// to the app the person was using. Clicking the island also brings it to the
/// front; that focus is kept until the island rests again (see `Island.tsx`).
#[cfg(windows)]
pub fn set_focused(app: &AppHandle, focused: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let Some(raw) = island_hwnd(app) else { return };
    let hwnd = HWND(raw as _);
    let state = app.state::<IslandState>();
    let Ok(mut s) = state.0.lock() else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if focused {
            if s.focused {
                return;
            }
            s.focused = true;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            fg::force_activate(raw);
            let _ = win.set_focus();
        } else {
            if std::mem::replace(&mut s.focused, false) {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
            }
            // A click on the island makes it the foreground window. Hand
            // focus back only if it still is: if the person already clicked
            // into another app, leave that alone.
            if fg::current() == raw {
                fg::activate(s.last_external);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn set_focused(_app: &AppHandle, _focused: bool) {}

/// Ends typing mode and makes sure the person's app is in front (paste target).
pub fn release_focus(app: &AppHandle) {
    set_focused(app, false);
    #[cfg(windows)]
    if let Ok(s) = app.state::<IslandState>().0.lock() {
        fg::activate(s.last_external);
    }
}

/// Remembers the last foreground window outside Duri: where focus goes back
/// to when the island lets go of it, and where pasting goes.
#[cfg(windows)]
fn track_foreground(s: &mut Inner) {
    let current = fg::current();
    if current != 0 && !fg::is_ours(current) {
        s.last_external = current;
    }
}

#[tauri::command]
pub fn island_set_focus(app: AppHandle, focused: bool) {
    set_focused(&app, focused);
}

pub fn spawn_hit_test(app: AppHandle) {
    std::thread::Builder::new()
        .name("duri-hit-test".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(20));
            let Some(win) = app.get_webview_window(LABEL) else {
                continue;
            };
            let (Ok(cursor), Ok(origin), Ok(scale)) =
                (win.cursor_position(), win.outer_position(), win.scale_factor())
            else {
                continue;
            };
            let lx = (cursor.x - origin.x as f64) / scale;
            let ly = (cursor.y - origin.y as f64) / scale;

            let state = app.state::<IslandState>();
            let Ok(mut s) = state.0.lock() else { continue };
            #[cfg(windows)]
            track_foreground(&mut s);
            let inside = !s.hidden && s.rects.iter().any(|r| r.contains(lx, ly));
            if inside != s.inside {
                s.inside = inside;
                drop(s);
                let _ = win.set_ignore_cursor_events(!inside);
                let _ = win.emit_to(LABEL, "island://pointer", inside);
            }
        })
        .expect("failed to spawn hit-test thread");
}

/// Whether the pointer is over the island right now (the UI asks on load,
/// since a reloaded page would otherwise miss the last pointer event).
#[tauri::command]
pub fn island_pointer(state: State<'_, IslandState>) -> bool {
    state.0.lock().map(|s| s.inside).unwrap_or(false)
}

/// Rectangles are in logical pixels relative to the window's top-left corner.
#[tauri::command]
pub fn island_set_hit_rects(state: State<'_, IslandState>, rects: Vec<Rect>) {
    if let Ok(mut s) = state.0.lock() {
        s.rects = rects;
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn rect_contains_edges() {
        let r = Rect { x: 10.0, y: 10.0, w: 20.0, h: 10.0 };
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(30.0, 20.0));
        assert!(!r.contains(30.1, 15.0));
        assert!(!r.contains(15.0, 9.9));
    }
}
