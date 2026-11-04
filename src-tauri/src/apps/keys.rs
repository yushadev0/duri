//! Lock keys app: Caps Lock, Insert and Scroll Lock show their new state in
//! the island for a moment, beside the volume indicator.
//!
//! A low-level keyboard hook notices the key press (and never swallows it).
//! The toggle state is read a moment later on another thread with
//! `GetKeyState`, because inside the hook Windows hasn't flipped it yet.

use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LockKey {
    Caps,
    Insert,
    Scroll,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockEvent {
    pub key: LockKey,
    pub on: bool,
}

/// Current toggle state of each lock key.
#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct LockStates {
    pub caps: bool,
    pub insert: bool,
    pub scroll: bool,
}

#[tauri::command]
pub fn keys_get() -> LockStates {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL, VK_INSERT, VK_SCROLL};
        let on = |vk: u16| unsafe { GetKeyState(vk as i32) } & 1 != 0;
        LockStates { caps: on(VK_CAPITAL.0), insert: on(VK_INSERT.0), scroll: on(VK_SCROLL.0) }
    }
    #[cfg(not(windows))]
    LockStates::default()
}

pub fn init(app: &AppHandle) {
    #[cfg(windows)]
    hook::install(app.clone());
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod hook {
    use super::{LockEvent, LockKey};
    use std::sync::mpsc::{self, Sender};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL, VK_INSERT, VK_SCROLL};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, MSG,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    static TX: OnceLock<Sender<(LockKey, u16)>> = OnceLock::new();
    /// The lock key currently held down, so auto-repeat doesn't count as presses.
    static HELD: AtomicU32 = AtomicU32::new(0);
    /// Windows flips the toggle state after the hook returns.
    const SETTLE: Duration = Duration::from_millis(40);

    fn lock_key(vk: u32) -> Option<(LockKey, u16)> {
        match vk {
            k if k == VK_CAPITAL.0 as u32 => Some((LockKey::Caps, VK_CAPITAL.0)),
            k if k == VK_INSERT.0 as u32 => Some((LockKey::Insert, VK_INSERT.0)),
            k if k == VK_SCROLL.0 as u32 => Some((LockKey::Scroll, VK_SCROLL.0)),
            _ => None,
        }
    }

    unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if let Some(lock) = lock_key(key.vkCode) {
                let msg = wparam.0 as u32;
                if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                    if HELD.swap(key.vkCode, Ordering::Relaxed) != key.vkCode
                        && let Some(tx) = TX.get()
                    {
                        let _ = tx.send(lock);
                    }
                } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
                    HELD.store(0, Ordering::Relaxed);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    pub fn install(app: AppHandle) {
        let (tx, rx) = mpsc::channel::<(LockKey, u16)>();
        let _ = TX.set(tx);
        std::thread::Builder::new()
            .name("duri-lock-keys".into())
            .spawn(move || {
                while let Ok((key, vk)) = rx.recv() {
                    std::thread::sleep(SETTLE);
                    let on = unsafe { GetKeyState(vk as i32) } & 1 != 0;
                    let _ = app.emit("keys://lock", LockEvent { key, on });
                }
            })
            .expect("failed to spawn lock keys thread");
        // Low-level hooks are called on the installing thread, which must pump messages.
        std::thread::Builder::new()
            .name("duri-lock-keys-hook".into())
            .spawn(|| unsafe {
                let module = GetModuleHandleW(None).ok();
                if SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module.map(Into::into), 0).is_err() {
                    log::error!("lock key hook could not be installed");
                    return;
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("failed to spawn lock keys hook thread");
    }
}
