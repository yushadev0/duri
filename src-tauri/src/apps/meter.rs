//! Output level for Music › Idle Mode's sound-reactive circle: the peak of
//! whatever the default speakers are playing (`IAudioMeterInformation`, the
//! same meter the Windows volume mixer shows). No audio is captured; only one
//! number is read, ~30 times a second, and only while Idle Mode asks for it.
//!
//! `audio://level` (0.0 – 1.0) goes to the idle window only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_millis(33);
/// The default device may change (headphones plugged in): look it up again this often.
const REOPEN: Duration = Duration::from_secs(3);

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn start(app: &AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("duri-meter".into()).spawn(move || worker(app));
    if spawned.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
}

#[cfg(windows)]
fn worker(app: AppHandle) {
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

    let open = || -> windows::core::Result<IAudioMeterInformation> {
        unsafe {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?.Activate(CLSCTX_ALL, None)
        }
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut meter = None;
    let mut opened = Instant::now() - REOPEN;
    let mut last = -1.0f32;
    while RUNNING.load(Ordering::SeqCst) {
        if opened.elapsed() >= REOPEN {
            meter = open().ok();
            opened = Instant::now();
        }
        let level = meter.as_ref().and_then(|m| unsafe { m.GetPeakValue().ok() }).unwrap_or(0.0);
        // Silence repeats; only send it once.
        if level > 0.0 || last != 0.0 {
            let _ = app.emit_to(crate::idle_window::LABEL, "audio://level", level);
        }
        last = level;
        std::thread::sleep(TICK);
    }
    drop(meter);
    unsafe { CoUninitialize() };
}

#[cfg(not(windows))]
fn worker(_app: AppHandle) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn audio_meter(app: AppHandle, on: bool) {
    if on { start(&app) } else { stop() }
}
