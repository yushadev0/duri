//! Getting `duri_clock.dll` (src-clock/clock.cpp) into explorer and talking
//! to it. The DLL is embedded in Duri and written next to the database under
//! a name that carries its hash: a loaded DLL is locked, so an update must
//! never need to overwrite one, and the DLL names its command window after
//! its own file, so we only ever talk to the exact build we shipped.

use std::path::{Path, PathBuf};
use windows::core::{s, w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowThreadProcessId, RegisterWindowMessageW, SendMessageTimeoutW,
    SetWindowsHookExW, UnhookWindowsHookEx, HOOKPROC, HWND_MESSAGE, SMTO_ABORTIFHUNG, WH_CALLWNDPROC, WM_COPYDATA,
};

static DLL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/duri_clock.dll"));

// Keep in sync with src-clock/clock.cpp.
const HOST_CLASS_PREFIX: &str = "DuriClockHost:";
const CMD_REVERT: usize = 0;
const CMD_SET_TEXT: usize = 1;
const SEND_TIMEOUT_MS: u32 = 1000;

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The taskbar window and the id of the explorer process that owns it.
pub fn taskbar() -> Option<(HWND, u32, u32)> {
    unsafe {
        let window = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let mut process = 0u32;
        let thread = GetWindowThreadProcessId(window, Some(&mut process));
        (thread != 0).then_some((window, thread, process))
    }
}

pub struct Injector {
    path: PathBuf,
    class: HSTRING,
    module: Option<HMODULE>,
}

impl Injector {
    /// Writes the DLL into `dir` (and clears out the ones older versions left
    /// there, as far as explorer has let go of them).
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        let stem = format!("duri_clock_{:016x}", fnv1a(DLL));
        let folder = dir.join("clock");
        std::fs::create_dir_all(&folder)?;
        let path = folder.join(format!("{stem}.dll"));

        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                if entry.path() != path {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let current = std::fs::metadata(&path).map(|meta| meta.len()).ok();
        if current != Some(DLL.len() as u64) {
            std::fs::write(&path, DLL)?;
        }
        Ok(Self { path, class: HSTRING::from(format!("{HOST_CLASS_PREFIX}{stem}")), module: None })
    }

    /// The DLL's command window, if it is loaded in explorer.
    pub fn host(&self) -> Option<HWND> {
        unsafe { FindWindowExW(Some(HWND_MESSAGE), None, &self.class, PCWSTR::null()).ok() }
    }

    /// Loads the DLL into the taskbar thread: a thread hook makes Windows
    /// map it there, and the attach message makes it stay.
    pub fn inject(&mut self, taskbar: HWND, thread: u32) -> Result<(), String> {
        unsafe {
            let module = match self.module {
                Some(module) => module,
                None => {
                    let module = LoadLibraryW(&HSTRING::from(self.path.as_os_str())).map_err(|e| e.to_string())?;
                    self.module = Some(module);
                    module
                }
            };
            let entry = GetProcAddress(module, s!("DuriHookProc")).ok_or("DuriHookProc is missing")?;
            // SAFETY: DuriHookProc is declared as a HOOKPROC in clock.cpp.
            let hook_proc: HOOKPROC = Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(i32, WPARAM, LPARAM) -> windows::Win32::Foundation::LRESULT,
            >(entry));
            let hook =
                SetWindowsHookExW(WH_CALLWNDPROC, hook_proc, Some(module.into()), thread).map_err(|e| e.to_string())?;
            let attach = RegisterWindowMessageW(w!("DuriClockAttach"));
            let sent = SendMessageTimeoutW(taskbar, attach, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 3000, None);
            let _ = UnhookWindowsHookEx(hook);
            if sent.0 == 0 {
                return Err("the taskbar did not answer".into());
            }
        }
        // The command window is created inside that message; allow a moment
        // in case the taskbar was busy.
        for _ in 0..20 {
            if self.host().is_some() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err("the clock helper did not start".into())
    }
}

fn send(host: HWND, command: usize, payload: &[u8]) -> Option<usize> {
    let data = COPYDATASTRUCT {
        dwData: command,
        cbData: payload.len() as u32,
        lpData: payload.as_ptr().cast_mut().cast(),
    };
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            host,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&data as *const COPYDATASTRUCT as isize),
            SMTO_ABORTIFHUNG,
            SEND_TIMEOUT_MS,
            Some(&mut result),
        )
    };
    (sent.0 != 0 && result != 0).then_some(result)
}

/// Shows `text` (lines separated by `\n`) in the clock. `align` is 0 left,
/// 1 center, 2 right; a new `generation` tells the DLL the lines were edited.
/// Returns how many taskbar clocks took it, or `None` if the DLL could not be
/// reached.
pub fn set_text(host: HWND, text: &str, align: u32, generation: u32) -> Option<usize> {
    // SetHeader in clock.cpp. Our process id first: the DLL restores the
    // Windows clock if we die.
    let mut payload = Vec::with_capacity(12 + 2 * (text.len() + 1));
    for field in [std::process::id(), align, generation] {
        payload.extend(field.to_le_bytes());
    }
    payload.extend(text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes));
    send(host, CMD_SET_TEXT, &payload).map(|result| result - 1)
}

/// Brings the Windows clock back.
pub fn revert(host: HWND) {
    send(host, CMD_REVERT, &[]);
}

#[cfg(test)]
mod tests {
    use super::fnv1a;

    #[test]
    fn fnv1a_known_values() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
