//! CPU and memory usage for the idle pill, sampled every 2 seconds.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// 0 – 100
    pub cpu: u8,
    /// 0 – 100
    pub memory: u8,
}

#[derive(Default)]
pub struct SystemStats(Mutex<Stats>);

/// CPU busy share between two `(idle, kernel, user)` samples. Kernel time
/// includes idle time, as reported by GetSystemTimes.
pub fn cpu_percent(prev: (u64, u64, u64), now: (u64, u64, u64)) -> u8 {
    let idle = now.0.saturating_sub(prev.0);
    let total = now.1.saturating_sub(prev.1) + now.2.saturating_sub(prev.2);
    if total == 0 {
        return 0;
    }
    (100.0 * (1.0 - idle as f64 / total as f64)).round().clamp(0.0, 100.0) as u8
}

#[cfg(windows)]
mod sample {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;

    fn ticks(f: FILETIME) -> u64 {
        ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
    }

    pub fn times() -> Option<(u64, u64, u64)> {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
        Some((ticks(idle), ticks(kernel), ticks(user)))
    }

    pub fn memory() -> Option<u8> {
        let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        unsafe { GlobalMemoryStatusEx(&mut status).ok()? };
        Some(status.dwMemoryLoad as u8)
    }
}

pub fn init(app: &AppHandle) {
    app.manage(SystemStats::default());
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::Builder::new()
            .name("duri-system-stats".into())
            .spawn(move || {
                let mut prev = sample::times();
                loop {
                    std::thread::sleep(INTERVAL);
                    let now = sample::times();
                    let cpu = match (prev, now) {
                        (Some(p), Some(n)) => cpu_percent(p, n),
                        _ => 0,
                    };
                    prev = now;
                    let stats = Stats { cpu, memory: sample::memory().unwrap_or(0) };
                    let state = app.state::<SystemStats>();
                    let changed = state.0.lock().map(|mut s| std::mem::replace(&mut *s, stats) != stats).unwrap_or(false);
                    if changed {
                        let _ = app.emit("system://stats", stats);
                    }
                }
            })
            .expect("failed to spawn system stats thread");
    }
}

#[tauri::command]
pub fn system_stats(state: State<'_, SystemStats>) -> Stats {
    state.0.lock().map(|s| *s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::cpu_percent;

    #[test]
    fn cpu_share_from_deltas() {
        // 100 ticks passed (kernel 60 incl. 50 idle, user 40): 50% busy.
        assert_eq!(cpu_percent((0, 0, 0), (50, 60, 40)), 50);
        assert_eq!(cpu_percent((0, 0, 0), (100, 100, 0)), 0);
        assert_eq!(cpu_percent((5, 5, 5), (5, 5, 5)), 0);
    }
}
