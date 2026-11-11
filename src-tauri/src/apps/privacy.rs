//! Privacy app: shows when an app is using the camera or the microphone.
//!
//! Windows records every use under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{webcam,microphone}`:
//! one subkey per packaged app, and one per desktop app under `NonPackaged`
//! (named after the exe path with `\` replaced by `#`). Each has
//! `LastUsedTimeStart` and `LastUsedTimeStop` (FILETIMEs); a stop of 0 means
//! the app is using the device right now. This is what drives the Windows
//! taskbar's own camera and microphone icons.
//!
//! `RegNotifyChangeKeyValue` on both trees wakes the worker on any change, so
//! nothing is polled while nothing happens.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InUse {
    /// Display names of the apps using the camera, sorted.
    pub camera: Vec<String>,
    /// Display names of the apps using the microphone, sorted.
    pub microphone: Vec<String>,
}

#[derive(Default)]
pub struct Privacy(Mutex<InUse>);

pub fn init(app: &AppHandle) {
    app.manage(Privacy::default());
    #[cfg(windows)]
    watch::spawn(app.clone());
}

#[tauri::command]
pub fn privacy_get(privacy: State<'_, Privacy>) -> Result<InUse, String> {
    Ok(privacy.0.lock().map_err(|e| e.to_string())?.clone())
}

/// A readable name for a packaged app's family name:
/// `Microsoft.WindowsCamera_8wekyb3d8bbwe` → "Windows Camera",
/// `MSTeams_8wekyb3d8bbwe` → "MS Teams".
pub fn packaged_name(family: &str) -> String {
    let name = family.split('_').next().unwrap_or(family);
    let name = name.rsplit('.').next().unwrap_or(name);
    split_words(name)
}

/// Splits CamelCase into words, keeping runs of capitals together
/// ("MSTeams" → "MS Teams", "WindowsCamera" → "Windows Camera").
fn split_words(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || (prev.is_uppercase() && next_lower) {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

/// The exe path a `NonPackaged` subkey stands for (`C:#Program Files#…#app.exe`).
pub fn nonpackaged_path(key: &str) -> String {
    key.replace('#', "\\")
}

/// The exe's file name without extension: the fallback display name.
pub fn exe_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file).to_string()
}

#[cfg(windows)]
mod watch {
    use super::{exe_stem, nonpackaged_path, packaged_name, InUse, Privacy};
    use std::collections::HashMap;
    use tauri::{AppHandle, Emitter, Manager};
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, WAIT_FAILED};
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME, RRF_RT_REG_QWORD,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};

    const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
    /// A safety re-read, in case a notification was missed.
    const SAFETY_MS: u32 = 60_000;

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    fn open(parent: HKEY, path: &str) -> Option<Key> {
        let mut key = HKEY::default();
        let path = HSTRING::from(path);
        let ok = unsafe { RegOpenKeyExW(parent, &path, None, KEY_READ | KEY_NOTIFY, &mut key) };
        (ok == ERROR_SUCCESS).then_some(Key(key))
    }

    fn subkeys(key: &Key) -> Vec<String> {
        let mut names = Vec::new();
        let mut buf = [0u16; 512];
        for i in 0.. {
            let mut len = buf.len() as u32;
            let r = unsafe {
                RegEnumKeyExW(key.0, i, Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None)
            };
            if r != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        names
    }

    fn qword(key: &Key, name: &str) -> Option<u64> {
        let mut value = 0u64;
        let mut size = 8u32;
        let name = HSTRING::from(name);
        let r = unsafe {
            RegGetValueW(
                key.0,
                PCWSTR::null(),
                &name,
                RRF_RT_REG_QWORD,
                None,
                Some(&mut value as *mut u64 as *mut _),
                Some(&mut size),
            )
        };
        (r == ERROR_SUCCESS).then_some(value)
    }

    /// Started and not stopped yet.
    fn in_use(key: &Key) -> bool {
        qword(key, "LastUsedTimeStart").is_some_and(|s| s != 0) && qword(key, "LastUsedTimeStop") == Some(0)
    }

    /// The exe's FileDescription ("Google Chrome"), or its file name.
    fn exe_name(path: &str, cache: &mut HashMap<String, String>) -> String {
        if let Some(n) = cache.get(path) {
            return n.clone();
        }
        let name = file_description(path).unwrap_or_else(|| exe_stem(path));
        cache.insert(path.to_string(), name.clone());
        name
    }

    fn file_description(path: &str) -> Option<String> {
        let path = HSTRING::from(path);
        unsafe {
            let size = GetFileVersionInfoSizeW(&path, None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(&path, None, size, data.as_mut_ptr() as *mut _).ok()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            if !VerQueryValueW(data.as_ptr() as *const _, &HSTRING::from(r"\VarFileInfo\Translation"), &mut ptr, &mut len)
                .as_bool()
                || len < 4
            {
                return None;
            }
            let lang = *(ptr as *const u16);
            let page = *(ptr as *const u16).add(1);
            let query = HSTRING::from(format!(r"\StringFileInfo\{lang:04x}{page:04x}\FileDescription"));
            if !VerQueryValueW(data.as_ptr() as *const _, &query, &mut ptr, &mut len).as_bool() || len == 0 {
                return None;
            }
            let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let text = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
            (!text.is_empty()).then_some(text)
        }
    }

    fn users(store: &Key, cache: &mut HashMap<String, String>) -> Vec<String> {
        let mut names = Vec::new();
        for sub in subkeys(store) {
            let Some(key) = open(store.0, &sub) else { continue };
            if sub == "NonPackaged" {
                for exe in subkeys(&key) {
                    if let Some(k) = open(key.0, &exe)
                        && in_use(&k)
                    {
                        names.push(exe_name(&nonpackaged_path(&exe), cache));
                    }
                }
            } else if in_use(&key) {
                names.push(packaged_name(&sub));
            }
        }
        names.sort();
        names.dedup();
        names
    }

    pub fn spawn(app: AppHandle) {
        std::thread::Builder::new()
            .name("duri-privacy".into())
            .spawn(move || {
                let (Some(camera), Some(mic)) =
                    (open(HKEY_CURRENT_USER, &format!(r"{ROOT}\webcam")), open(HKEY_CURRENT_USER, &format!(r"{ROOT}\microphone")))
                else {
                    log::warn!("camera/microphone usage keys not found");
                    return;
                };
                let make = || unsafe { CreateEventW(None, false, false, None) };
                let (Ok(on_camera), Ok(on_mic)) = (make(), make()) else { return };
                let events: [HANDLE; 2] = [on_camera, on_mic];
                let filter = REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME;
                let mut cache = HashMap::new();
                let mut last = InUse::default();
                loop {
                    // Re-arm before reading, so a change during the read isn't lost.
                    unsafe {
                        let _ = RegNotifyChangeKeyValue(camera.0, true, filter, Some(events[0]), true);
                        let _ = RegNotifyChangeKeyValue(mic.0, true, filter, Some(events[1]), true);
                    }
                    let now = InUse { camera: users(&camera, &mut cache), microphone: users(&mic, &mut cache) };
                    if now != last {
                        if let Ok(mut s) = app.state::<Privacy>().0.lock() {
                            *s = now.clone();
                        }
                        let _ = app.emit("privacy://changed", &now);
                        last = now;
                    }
                    if unsafe { WaitForMultipleObjects(&events, false, SAFETY_MS) } == WAIT_FAILED {
                        return;
                    }
                }
            })
            .expect("failed to spawn privacy thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_names_read_like_names() {
        assert_eq!(packaged_name("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "Windows Camera");
        assert_eq!(packaged_name("MSTeams_8wekyb3d8bbwe"), "MS Teams");
        assert_eq!(packaged_name("Microsoft.ScreenSketch_8wekyb3d8bbwe"), "Screen Sketch");
        assert_eq!(packaged_name("Zoom"), "Zoom");
    }

    #[test]
    fn nonpackaged_keys_are_exe_paths() {
        let path = nonpackaged_path("C:#Program Files#Google#Chrome#Application#chrome.exe");
        assert_eq!(path, r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        assert_eq!(exe_stem(&path), "chrome");
        assert_eq!(exe_stem(r"C:\Games\How to Fish.exe"), "How to Fish");
    }
}
