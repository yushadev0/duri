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
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Hidden from the tray menu.
    hidden: bool,
    /// Hidden because a fullscreen app is in front (Settings › Hide in Fullscreen).
    fullscreen: bool,
    /// That fullscreen app is an exclusive-fullscreen game or a presentation:
    /// nothing may draw over it.
    exclusive: bool,
    /// A short-lived indicator (volume, Caps Lock…) is showing. It shows even
    /// over a fullscreen app, like the Windows volume popup does.
    hud: bool,
    /// Shown over a fullscreen app for an indicator, bypassing Tauri's own
    /// show (which may activate the window and pull the app out of focus).
    over_fullscreen: bool,
    /// Whether the window is currently shown.
    shown: bool,
    /// The island currently has keyboard focus (someone is typing in it).
    focused: bool,
    /// Last foreground window that isn't Duri's (raw HWND): where focus
    /// returns and where pasting goes.
    last_external: isize,
}

#[derive(Default)]
pub struct IslandState(Mutex<Inner>);

/// Settings › Hide in Fullscreen.
static HIDE_IN_FULLSCREEN: AtomicBool = AtomicBool::new(true);

pub fn set_hide_in_fullscreen(on: bool) {
    HIDE_IN_FULLSCREEN.store(on, Ordering::SeqCst);
}

impl Inner {
    fn visible(&self) -> bool {
        !self.hidden && (!self.fullscreen || (self.hud && !self.exclusive))
    }
}

impl IslandState {
    /// Whether the island is on screen (not hidden from the tray or by a
    /// fullscreen app).
    pub fn is_visible(&self) -> bool {
        self.0.lock().map(|s| s.visible()).unwrap_or(false)
    }
}

/// Hides the island from the tray menu, or shows it again.
pub fn set_hidden(app: &AppHandle, hidden: bool) {
    if let Ok(mut s) = app.state::<IslandState>().0.lock() {
        s.hidden = hidden;
    }
    apply_visibility(app);
}

/// Shows or hides the window to match the tray, fullscreen and indicator state.
fn apply_visibility(app: &AppHandle) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let (show, over_fullscreen) = {
        let state = app.state::<IslandState>();
        let Ok(mut s) = state.0.lock() else { return };
        let show = s.visible();
        if show == s.shown {
            return;
        }
        s.shown = show;
        let fullscreen = s.fullscreen;
        let raw = std::mem::replace(&mut s.over_fullscreen, show && fullscreen);
        (show, if show { fullscreen } else { raw })
    };
    match (show, over_fullscreen) {
        #[cfg(windows)]
        (true, true) => show_over_fullscreen(&win),
        (true, _) => show_without_activating(&win),
        #[cfg(windows)]
        (false, true) => hide_raw(&win),
        (false, _) => {
            let _ = win.hide();
        }
    }
}

/// Shows the island above a fullscreen app without activating it at all:
/// even a brief focus change can minimize a borderless game.
#[cfg(windows)]
fn show_over_fullscreen(win: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    if let Ok(h) = win.hwnd() {
        unsafe {
            let _ = SetWindowPos(
                HWND(h.0 as _),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

/// Hides a window shown by `show_over_fullscreen`. Tauri didn't see that
/// show, so its own `hide` would think there is nothing to do.
#[cfg(windows)]
fn hide_raw(win: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    if let Ok(h) = win.hwnd() {
        unsafe {
            let _ = ShowWindow(HWND(h.0 as _), SW_HIDE);
        }
    }
}

/// The UI says whether an indicator (volume, Caps Lock…) is showing.
#[tauri::command]
pub fn island_set_hud(app: AppHandle, on: bool) {
    let changed = app
        .state::<IslandState>()
        .0
        .lock()
        .map(|mut s| std::mem::replace(&mut s.hud, on) != on)
        .unwrap_or(false);
    if changed {
        apply_visibility(&app);
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
    apply_visibility(app);
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

/// Where the window belongs on a monitor, in physical pixels: horizontally
/// centered at the very top. Returns `(x, y, width, height)`.
pub fn placement(monitor_pos: (i32, i32), monitor_width: u32, scale: f64) -> (i32, i32, u32, u32) {
    let w = (WIDTH * scale).round() as i32;
    let h = (HEIGHT * scale).round() as i32;
    let x = monitor_pos.0 + (monitor_width as i32 - w) / 2;
    (x, monitor_pos.1, w as u32, h as u32)
}

/// Centers the window horizontally at the very top of the primary monitor.
pub fn position(win: &WebviewWindow) -> tauri::Result<()> {
    if let Some(m) = win.primary_monitor()? {
        let (x, y, w, h) = placement((m.position().x, m.position().y), m.size().width, m.scale_factor());
        win.set_size(tauri::PhysicalSize::new(w, h))?;
        win.set_position(PhysicalPosition::new(x, y))?;
    }
    Ok(())
}

/// Puts the window back in place if it has drifted: Duri often starts with
/// Windows sign-in, before the display's resolution and scale have settled,
/// and resolution, scale or primary-monitor changes later move it too.
fn keep_in_place(win: &WebviewWindow) {
    let (Ok(Some(m)), Ok(pos), Ok(size)) = (win.primary_monitor(), win.outer_position(), win.outer_size()) else {
        return;
    };
    let (x, y, w, h) = placement((m.position().x, m.position().y), m.size().width, m.scale_factor());
    if (pos.x, pos.y, size.width, size.height) != (x, y, w, h) {
        let _ = position(win);
    }
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

/// Brings another app's window (e.g. the music player) to the front from a
/// click in the island, restoring it if minimized. It also becomes the window
/// focus is handed back to, so the island doesn't pull the old one forward.
#[cfg(windows)]
pub fn activate_external(app: &AppHandle, h: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_RESTORE};
    if h == 0 {
        return;
    }
    if let Ok(mut s) = app.state::<IslandState>().0.lock() {
        s.last_external = h;
    }
    unsafe {
        if IsIconic(HWND(h as _)).as_bool() {
            let _ = ShowWindow(HWND(h as _), SW_RESTORE);
        }
    }
    fg::force_activate(h);
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

/// How often the cursor is checked while it is near the island, and while
/// it is elsewhere on screen.
const HIT_TEST_NEAR: Duration = Duration::from_millis(20);
const HIT_TEST_FAR: Duration = Duration::from_millis(60);
/// "Near" = within this many logical px of the island window.
const NEAR_MARGIN: f64 = 80.0;
/// The window's position and scale only change on display changes; re-read
/// them (and put the window back in place) this often instead of every tick.
const GEOMETRY_REFRESH: Duration = Duration::from_secs(1);

/// Cursor position in physical screen pixels.
#[cfg(windows)]
fn cursor_pos() -> Option<(f64, f64)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x as f64, p.y as f64))
}

#[cfg(not(windows))]
fn cursor_pos() -> Option<(f64, f64)> {
    None
}

/// Whether a point (logical px, relative to the window) is within `margin`
/// of the window's bounds.
pub fn near_window(x: f64, y: f64, margin: f64) -> bool {
    x >= -margin && y >= -margin && x <= WIDTH + margin && y <= HEIGHT + margin
}

pub fn spawn_hit_test(app: AppHandle) {
    std::thread::Builder::new()
        .name("duri-hit-test".into())
        .spawn(move || {
            let mut geometry: Option<((f64, f64), f64, std::time::Instant)> = None;
            let mut interval = HIT_TEST_NEAR;
            loop {
                std::thread::sleep(interval);
                let Some(win) = app.get_webview_window(LABEL) else {
                    continue;
                };
                let stale = geometry.is_none_or(|(_, _, at)| at.elapsed() >= GEOMETRY_REFRESH);
                if stale {
                    keep_in_place(&win);
                }
                if stale && let (Ok(origin), Ok(scale)) = (win.outer_position(), win.scale_factor()) {
                    geometry = Some(((origin.x as f64, origin.y as f64), scale, std::time::Instant::now()));
                }
                let (Some(((ox, oy), scale, _)), Some((cx, cy))) = (geometry, cursor_pos()) else {
                    continue;
                };
                let lx = (cx - ox) / scale;
                let ly = (cy - oy) / scale;

                let state = app.state::<IslandState>();
                let Ok(mut s) = state.0.lock() else { continue };
                #[cfg(windows)]
                track_foreground(&mut s);
                let inside = s.visible() && s.rects.iter().any(|r| r.contains(lx, ly));
                interval = if inside || near_window(lx, ly, NEAR_MARGIN) { HIT_TEST_NEAR } else { HIT_TEST_FAR };
                if inside != s.inside {
                    s.inside = inside;
                    drop(s);
                    let _ = win.set_ignore_cursor_events(!inside);
                    let _ = win.emit_to(LABEL, "island://pointer", inside);
                }
            }
        })
        .expect("failed to spawn hit-test thread");
}

/// Window bounds as `(left, top, right, bottom)` in physical pixels.
type Bounds = (i32, i32, i32, i32);

/// WS_CAPTION: a title bar. Maximized windows keep it; fullscreen ones drop it.
const WS_CAPTION: u32 = 0x00C0_0000;

/// Whether a window is fullscreen on its monitor: it covers the whole monitor
/// (taskbar included) and has no title bar. Browsers report a fullscreen
/// video or F11 as still maximized, so the maximized flag can't decide it;
/// an ordinary maximized window keeps its caption, and with a visible
/// taskbar doesn't reach the bottom edge anyway.
pub fn is_fullscreen(window: Bounds, monitor: Bounds, style: u32) -> bool {
    let covers = window.0 <= monitor.0 && window.1 <= monitor.1 && window.2 >= monitor.2 && window.3 >= monitor.3;
    covers && style & WS_CAPTION != WS_CAPTION
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fullscreen {
    No,
    /// A borderless game, F11 browser or video player: an ordinary window
    /// covering the monitor, so the island can still draw over it.
    Window,
    /// Exclusive-fullscreen D3D or presentation mode.
    Exclusive,
}

/// Whether a fullscreen app, game or video is in front on the primary
/// monitor (where the island lives).
#[cfg(windows)]
fn fullscreen_in_front() -> Fullscreen {
    if exclusive_fullscreen() {
        Fullscreen::Exclusive
    } else if fullscreen_window_in_front() {
        Fullscreen::Window
    } else {
        Fullscreen::No
    }
}

/// Exclusive-fullscreen games and presentation mode, as Windows sees them.
#[cfg(windows)]
fn exclusive_fullscreen() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    matches!(
        unsafe { SHQueryUserNotificationState() },
        Ok(state) if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
    )
}

/// A foreground window covering the whole primary monitor.
#[cfg(windows)]
fn fullscreen_window_in_front() -> bool {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowLongPtrW, GetWindowRect, GWL_STYLE};

    let raw = fg::current();
    // Duri's own Idle Mode counts like any fullscreen player.
    if raw == 0 || (fg::is_ours(raw) && raw != crate::idle_window::hwnd()) {
        return false;
    }
    let hwnd = HWND(raw as _);
    unsafe {
        // The desktop itself covers the whole screen.
        let mut class = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() {
            return false;
        }
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut rect = RECT::default();
        if !GetMonitorInfoW(monitor, &mut info).as_bool() || GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let m = info.rcMonitor;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        // MONITORINFOF_PRIMARY: the island only lives on the primary monitor.
        info.dwFlags & 1 != 0
            && is_fullscreen((rect.left, rect.top, rect.right, rect.bottom), (m.left, m.top, m.right, m.bottom), style)
    }
}

/// Hides the island while a fullscreen app is in front, and brings it back
/// once it's gone.
pub fn spawn_fullscreen_watch(app: AppHandle) {
    #[cfg(windows)]
    std::thread::Builder::new()
        .name("duri-fullscreen".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let kind = if HIDE_IN_FULLSCREEN.load(Ordering::SeqCst) { fullscreen_in_front() } else { Fullscreen::No };
            let changed = app
                .state::<IslandState>()
                .0
                .lock()
                .map(|mut s| {
                    let (fullscreen, exclusive) = (kind != Fullscreen::No, kind == Fullscreen::Exclusive);
                    let changed = s.fullscreen != fullscreen || s.exclusive != exclusive;
                    s.fullscreen = fullscreen;
                    s.exclusive = exclusive;
                    changed
                })
                .unwrap_or(false);
            if changed {
                apply_visibility(&app);
            }
        })
        .expect("failed to spawn fullscreen watcher");
    #[cfg(not(windows))]
    let _ = app;
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
    use super::{is_fullscreen, near_window, placement, Rect};

    #[test]
    fn placed_top_center_of_the_monitor() {
        // 1920×1080 at 125%: an 800×400 window, centered.
        assert_eq!(placement((0, 0), 1920, 1.25), (560, 0, 800, 400));
        assert_eq!(placement((0, 0), 1920, 1.0), (640, 0, 640, 320));
        // A primary monitor that doesn't start at the origin.
        assert_eq!(placement((-2560, -200), 2560, 1.5), (-2560 + 800, -200, 960, 480));
    }

    #[test]
    fn near_the_window() {
        assert!(near_window(10.0, 10.0, 80.0));
        assert!(near_window(-60.0, 300.0, 80.0));
        assert!(!near_window(-100.0, 10.0, 80.0));
        assert!(!near_window(300.0, 500.0, 80.0));
    }

    // Measured on a 1920×1080 monitor with Edge.
    const MONITOR: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

    #[test]
    fn browser_fullscreen_counts_even_while_maximized() {
        // F11 / fullscreen video from a maximized window: style 0x170B0000.
        assert!(is_fullscreen((0, 0, 1920, 1080), MONITOR, 0x170B_0000));
    }

    #[test]
    fn maximized_window_is_not_fullscreen() {
        // Maximized Edge (style 0x17CF0000) stops at the taskbar.
        assert!(!is_fullscreen((-9, -9, 1929, 1029), MONITOR, 0x17CF_0000));
        // With an auto-hiding taskbar it covers the screen but keeps its caption.
        assert!(!is_fullscreen((-9, -9, 1929, 1089), MONITOR, 0x17CF_0000));
    }

    #[test]
    fn borderless_window_covering_the_screen_counts() {
        assert!(is_fullscreen((0, 0, 1920, 1080), MONITOR, 0x9600_0000));
        assert!(!is_fullscreen((0, 0, 1920, 1040), MONITOR, 0x9600_0000));
    }

    #[test]
    fn rect_contains_edges() {
        let r = Rect { x: 10.0, y: 10.0, w: 20.0, h: 10.0 };
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(30.0, 20.0));
        assert!(!r.contains(30.1, 15.0));
        assert!(!r.contains(15.0, 9.9));
    }
}
