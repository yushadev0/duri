//! Keeps every Duri window on the same WebView2 runtime.
//!
//! Duri runs for days. WebView2 updates itself in the background, and a new
//! webview picks the newest installed runtime. That runtime can't share the
//! user data folder with the older browser process the island still runs on,
//! so its browser process exits at once and the new window (Settings) closes
//! as soon as it opens. Pointing `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` at the
//! runtime the island already uses makes new webviews join that process. The
//! old runtime's folder stays on disk while that process is running. The next
//! start of Duri moves to the new version.

/// Call before building a new webview window. Does nothing if the folder is
/// already pinned or the island's browser process can't be found.
#[cfg(windows)]
pub fn pin_to_running() {
    const VAR: &str = "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER";
    if std::env::var_os(VAR).is_some() {
        return;
    }
    let Some(folder) = running_browser_folder() else { return };
    log::info!("pinning WebView2 runtime to {}", folder.display());
    // SAFETY: set once, before the window that needs it is built; nothing else
    // in Duri writes the environment. The WebView2 loader reads it on the main
    // thread when that window's webview is created, after this returns.
    unsafe { std::env::set_var(VAR, folder) };
}

#[cfg(not(windows))]
pub fn pin_to_running() {}

/// Folder of the `msedgewebview2.exe` that Duri started (its browser process,
/// the only direct child with that name).
#[cfg(windows)]
fn running_browser_folder() -> Option<std::path::PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let me = std::process::id();
    let pid = unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = None;
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if entry.th32ParentProcessID == me && name.eq_ignore_ascii_case("msedgewebview2.exe") {
                found = Some(entry.th32ProcessID);
                break;
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        found?
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let exe = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        exe.parent().map(|p| p.to_path_buf())
    }
}
