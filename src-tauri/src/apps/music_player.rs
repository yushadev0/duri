//! Opens the app behind the current music session (clicking the cover in
//! Music's expanded view).
//!
//! Windows only tells us the session's app id (AUMID): `Spotify.exe`,
//! `Chrome`, `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`… We look for a
//! top-level window of a process that matches it and bring it forward; when
//! a song's title is in a window's title (a browser tab, Spotify), that
//! window wins. A packaged app with no window found is activated through the
//! shell, which starts it or brings it back.

use super::music::Music;
use tauri::{AppHandle, State};

/// Removes the `#2` that marks a second session of the same app.
fn app_id(source_key: &str) -> &str {
    match source_key.rsplit_once('#') {
        Some((id, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => id,
        _ => source_key,
    }
}

fn stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let file = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
    file.to_ascii_lowercase()
}

/// Whether a process running `exe` (full path) is the app with this id.
pub fn matches(id: &str, exe: &str) -> bool {
    let exe_l = exe.to_ascii_lowercase();
    let exe_stem = stem(exe);
    if let Some((family, app)) = id.split_once('!') {
        // Packaged: installed under WindowsApps\<Name>_<Version>_<Arch>__<PublisherId>\.
        if let Some((name, publisher)) = family.rsplit_once('_') {
            let name = name.to_ascii_lowercase();
            let publisher = publisher.to_ascii_lowercase();
            if exe_l.contains(&format!("\\windowsapps\\{name}_")) && exe_l.contains(&format!("__{publisher}\\")) {
                return true;
            }
        }
        return stem(app) == exe_stem;
    }
    let id_l = id.to_ascii_lowercase();
    // Firefox reports a hash of its install folder instead of a name.
    if id_l.len() == 16 && id_l.bytes().all(|b| b.is_ascii_hexdigit()) {
        return exe_stem == "firefox";
    }
    stem(&id_l) == exe_stem || exe_l == id_l
}

#[cfg(windows)]
mod win {
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        GW_OWNER,
    };

    pub struct Candidate {
        pub hwnd: isize,
        pub title: String,
        pub exe: String,
    }

    fn exe_of(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(process);
            ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
        let out = unsafe { &mut *(data.0 as *mut Vec<Candidate>) };
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                return true.into();
            }
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return true.into();
            }
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, &mut buf);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == std::process::id() {
                return true.into();
            }
            if let Some(exe) = exe_of(pid) {
                out.push(Candidate {
                    hwnd: hwnd.0 as isize,
                    title: String::from_utf16_lossy(&buf[..n.max(0) as usize]),
                    exe,
                });
            }
        }
        true.into()
    }

    /// Visible, unowned, titled top-level windows of other apps, front to back.
    pub fn windows() -> Vec<Candidate> {
        let mut out: Vec<Candidate> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
        }
        out
    }
}

#[cfg(windows)]
fn open(app: &AppHandle, source_key: &str, track: &str) -> Result<(), String> {
    let id = app_id(source_key);
    if id.is_empty() {
        return Err("no player".into());
    }
    let track = track.to_lowercase();
    let found: Vec<win::Candidate> = win::windows().into_iter().filter(|c| matches(id, &c.exe)).collect();
    let best = found
        .iter()
        .find(|c| !track.is_empty() && c.title.to_lowercase().contains(&track))
        .or(found.first());
    if let Some(c) = best {
        crate::island_window::activate_external(app, c.hwnd);
        return Ok(());
    }
    if id.contains('!') {
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{id}"))
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(format!("no window for {id}"))
}

#[cfg(not(windows))]
fn open(_app: &AppHandle, _source_key: &str, _track: &str) -> Result<(), String> {
    Err("unsupported".into())
}

/// Brings the current session's player to the front.
#[tauri::command]
pub fn music_open_player(app: AppHandle, music: State<'_, Music>) -> Result<(), String> {
    let (key, title) = music.current_source();
    open(&app, &key, &title)
}

#[cfg(test)]
mod tests {
    use super::{app_id, matches};

    #[test]
    fn strips_session_number() {
        assert_eq!(app_id("Chrome#2"), "Chrome");
        assert_eq!(app_id("Spotify.exe"), "Spotify.exe");
        assert_eq!(app_id("a#b"), "a#b");
    }

    #[test]
    fn matches_players() {
        assert!(matches("Spotify.exe", r"C:\Users\x\AppData\Roaming\Spotify\Spotify.exe"));
        assert!(matches("Chrome", r"C:\Program Files\Google\Chrome\Application\chrome.exe"));
        assert!(matches("MSEdge", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"));
        assert!(matches("308046B0AF4A39CB", r"C:\Program Files\Mozilla Firefox\firefox.exe"));
        assert!(matches(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            r"C:\Program Files\WindowsApps\SpotifyAB.SpotifyMusic_1.250.0.0_x64__zpdnekdrzrea0\Spotify.exe"
        ));
        assert!(matches(
            "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
            r"C:\Program Files\WindowsApps\Microsoft.ZuneMusic_11.2410.0.0_x64__8wekyb3d8bbwe\Microsoft.Media.Player.exe"
        ));
        assert!(!matches("Chrome", r"C:\Program Files\Mozilla Firefox\firefox.exe"));
        assert!(!matches("Spotify.exe", r"C:\Windows\explorer.exe"));
    }
}
