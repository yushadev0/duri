//! Volume app: the system output volume, shown in the island when it changes.
//!
//! One worker thread owns the Core Audio endpoint of the default output
//! device. It polls the level every 100 ms (cheap, and it also catches changes
//! made by other apps and device switches) and applies commands sent from the
//! UI or from the keyboard hook.
//!
//! "Replace the Windows volume popup": Windows 11's flyout can't be hidden
//! reliably, so instead a low-level keyboard hook takes the volume keys,
//! changes the volume itself and swallows the key. Windows never sees the key
//! press, so its popup never opens, and Duri's indicator shows instead.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Matches Windows' own step for the volume keys.
pub const KEY_STEP: f32 = 0.02;
const POLL: Duration = Duration::from_millis(100);
const REFRESH_DEVICE: Duration = Duration::from_secs(2);

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    pub available: bool,
    /// 0.0 – 1.0
    pub level: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    Set(f32),
    Step(f32),
    ToggleMute,
}

/// New level after `cmd`, snapped to whole percents like Windows does.
pub fn next_level(level: f32, cmd: Cmd) -> f32 {
    let raw = match cmd {
        Cmd::Set(l) => l,
        Cmd::Step(d) => level + d,
        Cmd::ToggleMute => level,
    };
    (raw.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

/// Volume levels that differ by less than this are the same for display.
fn same(a: &VolumeState, b: &VolumeState) -> bool {
    a.available == b.available && a.muted == b.muted && (a.level - b.level).abs() < 0.004
}

pub struct Volume {
    tx: Mutex<Sender<Cmd>>,
    latest: Mutex<VolumeState>,
}

static HOOK_TX: OnceLock<Sender<Cmd>> = OnceLock::new();
static REPLACE_FLYOUT: AtomicBool = AtomicBool::new(false);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn init(app: &AppHandle, replace_flyout: bool) {
    let (tx, rx) = mpsc::channel();
    let _ = HOOK_TX.set(tx.clone());
    app.manage(Volume { tx: Mutex::new(tx), latest: Mutex::new(VolumeState::default()) });
    let app = app.clone();
    std::thread::Builder::new()
        .name("duri-volume".into())
        .spawn(move || worker(app, rx))
        .expect("failed to spawn volume thread");
    set_replace_flyout(replace_flyout);
}

/// Turns volume-key interception on or off (installs the hook the first time).
pub fn set_replace_flyout(on: bool) {
    REPLACE_FLYOUT.store(on, Ordering::SeqCst);
    if on && !HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        #[cfg(windows)]
        hook::install();
    }
}

#[cfg(windows)]
mod audio {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    pub fn init_com() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    pub fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
        unsafe {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
            device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
        }
    }

    pub fn read(ep: &IAudioEndpointVolume) -> windows::core::Result<(f32, bool)> {
        unsafe { Ok((ep.GetMasterVolumeLevelScalar()?, ep.GetMute()?.as_bool())) }
    }

    pub fn set_level(ep: &IAudioEndpointVolume, level: f32) -> windows::core::Result<()> {
        unsafe { ep.SetMasterVolumeLevelScalar(level, std::ptr::null()) }
    }

    pub fn set_mute(ep: &IAudioEndpointVolume, muted: bool) -> windows::core::Result<()> {
        unsafe { ep.SetMute(muted, std::ptr::null()) }
    }
}

#[cfg(windows)]
fn worker(app: AppHandle, rx: Receiver<Cmd>) {
    audio::init_com();
    let mut ep = None;
    let mut fetched = Instant::now() - REFRESH_DEVICE;
    let mut last = VolumeState::default();
    loop {
        let cmd = match rx.recv_timeout(POLL) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Follow the default device when it changes (headphones plugged in…).
        if ep.is_none() || fetched.elapsed() >= REFRESH_DEVICE {
            ep = audio::endpoint().ok();
            fetched = Instant::now();
        }
        let Some(endpoint) = ep.as_ref() else {
            if last.available {
                last = VolumeState::default();
                publish(&app, last);
            }
            continue;
        };
        if let Some(cmd) = cmd {
            let (level, muted) = audio::read(endpoint).unwrap_or((last.level, last.muted));
            let result = match cmd {
                Cmd::ToggleMute => audio::set_mute(endpoint, !muted),
                other => {
                    let target = next_level(level, other);
                    // Raising the volume unmutes, like the Windows keys do.
                    if muted && matches!(other, Cmd::Step(d) if d > 0.0) {
                        let _ = audio::set_mute(endpoint, false);
                    }
                    audio::set_level(endpoint, target)
                }
            };
            if result.is_err() {
                ep = None;
                continue;
            }
        }
        match audio::read(endpoint) {
            Ok((level, muted)) => {
                let now = VolumeState { available: true, level, muted };
                if !same(&now, &last) {
                    last = now;
                    publish(&app, now);
                }
            }
            Err(_) => ep = None,
        }
    }
}

#[cfg(not(windows))]
fn worker(_app: AppHandle, _rx: Receiver<Cmd>) {}

fn publish(app: &AppHandle, state: VolumeState) {
    if let Ok(mut l) = app.state::<Volume>().latest.lock() {
        *l = state;
    }
    let _ = app.emit("volume://changed", state);
}

#[cfg(windows)]
mod hook {
    use super::{Cmd, HOOK_TX, KEY_STEP, REPLACE_FLYOUT};
    use std::sync::atomic::Ordering;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, MSG,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && REPLACE_FLYOUT.load(Ordering::Relaxed) {
            let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let cmd = match key.vkCode {
                k if k == VK_VOLUME_UP.0 as u32 => Some(Cmd::Step(KEY_STEP)),
                k if k == VK_VOLUME_DOWN.0 as u32 => Some(Cmd::Step(-KEY_STEP)),
                k if k == VK_VOLUME_MUTE.0 as u32 => Some(Cmd::ToggleMute),
                _ => None,
            };
            if let Some(cmd) = cmd {
                let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
                if down && let Some(tx) = HOOK_TX.get() {
                    let _ = tx.send(cmd);
                }
                // Swallow both key down and key up so Windows never reacts.
                return LRESULT(1);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Low-level hooks are called on the installing thread, which must pump messages.
    pub fn install() {
        std::thread::Builder::new()
            .name("duri-volume-keys".into())
            .spawn(|| unsafe {
                let module = GetModuleHandleW(None).ok();
                if SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module.map(Into::into), 0).is_err() {
                    log::error!("volume key hook could not be installed");
                    return;
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("failed to spawn volume key thread");
    }
}

fn send(volume: &Volume, cmd: Cmd) -> Result<(), String> {
    volume.tx.lock().map_err(|e| e.to_string())?.send(cmd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn volume_get(volume: State<'_, Volume>) -> Result<VolumeState, String> {
    Ok(*volume.latest.lock().map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn volume_set(volume: State<'_, Volume>, level: f32) -> Result<(), String> {
    send(&volume, Cmd::Set(level))
}

#[tauri::command]
pub fn volume_toggle_mute(volume: State<'_, Volume>) -> Result<(), String> {
    send(&volume, Cmd::ToggleMute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_clamp_and_snap() {
        assert_eq!(next_level(0.5, Cmd::Step(KEY_STEP)), 0.52);
        assert_eq!(next_level(0.995, Cmd::Step(KEY_STEP)), 1.0);
        assert_eq!(next_level(0.01, Cmd::Step(-KEY_STEP)), 0.0);
        assert_eq!(next_level(0.3, Cmd::Set(0.4567)), 0.46);
        assert_eq!(next_level(0.3, Cmd::Set(7.0)), 1.0);
        assert_eq!(next_level(0.3, Cmd::ToggleMute), 0.3);
    }

    #[test]
    fn tiny_changes_are_not_news() {
        let a = VolumeState { available: true, level: 0.5, muted: false };
        assert!(same(&a, &VolumeState { level: 0.502, ..a }));
        assert!(!same(&a, &VolumeState { level: 0.52, ..a }));
        assert!(!same(&a, &VolumeState { muted: true, ..a }));
    }
}
