//! Battery and Brightness apps: the laptop's battery (charger plugged in or
//! out, low battery) and the screen brightness, shown in the island when
//! they change.
//!
//! Nothing is polled. A hidden window registers for Windows' power setting
//! notifications (`RegisterPowerSettingNotification`): power source, battery
//! percentage, battery saver, and the current monitor brightness. Windows
//! sends each one once right after registering (the starting values) and
//! again on every change. Battery details (charging, time left) are read with
//! `GetSystemPowerStatus` whenever one of the battery settings changes.
//!
//! Brightness is set through WMI (`WmiMonitorBrightnessMethods`), like the
//! Windows slider does for built-in panels. External monitors don't report
//! brightness this way, so desktops simply never show the indicator.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// The first notice: like Windows' own low battery level.
pub const LOW_BATTERY: u8 = 20;
/// The second notice, which holds the island open.
pub const CRITICAL_BATTERY: u8 = 10;

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatteryState {
    /// False on desktops (no system battery).
    pub present: bool,
    /// 0–100
    pub percent: u8,
    /// Running on AC power.
    pub plugged: bool,
    /// Plugged in and actually charging (not full).
    pub charging: bool,
    /// Time left on battery, when Windows has an estimate.
    pub seconds_left: Option<u32>,
    /// Battery saver is on.
    pub saver: bool,
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LowBattery {
    pub percent: u8,
    /// At or below `CRITICAL_BATTERY`.
    pub critical: bool,
}

#[derive(Serialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PowerSnapshot {
    pub battery: BatteryState,
    /// 0–100; None when the screen doesn't report its brightness.
    pub brightness: Option<u8>,
}

/// Decides when to warn about a low battery: once at `LOW_BATTERY`, once
/// more at `CRITICAL_BATTERY`, and again only after the charger was plugged
/// in. The first reading only records where things stand, so starting Duri
/// on an already low battery stays quiet until the next threshold.
#[derive(Default, Debug)]
pub struct LowWatch {
    primed: bool,
    /// The lowest threshold already warned about in this discharge.
    warned: Option<u8>,
}

impl LowWatch {
    pub fn update(&mut self, s: &BatteryState) -> Option<LowBattery> {
        let first = !std::mem::replace(&mut self.primed, true);
        if !s.present || s.plugged {
            self.warned = None;
            return None;
        }
        let threshold = if s.percent <= CRITICAL_BATTERY {
            CRITICAL_BATTERY
        } else if s.percent <= LOW_BATTERY {
            LOW_BATTERY
        } else {
            self.warned = None;
            return None;
        };
        if self.warned.is_some_and(|w| w <= threshold) {
            return None;
        }
        self.warned = Some(threshold);
        (!first).then_some(LowBattery { percent: s.percent, critical: threshold == CRITICAL_BATTERY })
    }
}

#[derive(Default)]
struct Inner {
    snapshot: PowerSnapshot,
    low: LowWatch,
    /// Whether the first battery reading arrived (plug changes after it are news).
    battery_known: bool,
}

#[derive(Default)]
pub struct Power(Mutex<Inner>);

pub fn init(app: &AppHandle) {
    app.manage(Power::default());
    #[cfg(windows)]
    win::spawn(app.clone());
}

/// A new battery reading: tells the UI about the state, a charger plugged in
/// or out, and a battery that just became low.
fn on_battery(app: &AppHandle, battery: BatteryState) {
    let (plug_change, low) = {
        let power = app.state::<Power>();
        let Ok(mut s) = power.0.lock() else { return };
        let previous = s.snapshot.battery;
        let known = std::mem::replace(&mut s.battery_known, true);
        if known && previous == battery {
            return;
        }
        s.snapshot.battery = battery;
        let plug_change = (known && battery.present && previous.plugged != battery.plugged).then_some(battery.plugged);
        (plug_change, s.low.update(&battery))
    };
    let _ = app.emit("power://battery", battery);
    if let Some(plugged) = plug_change {
        let _ = app.emit("power://plugged", plugged);
    }
    if let Some(low) = low {
        let _ = app.emit("power://low", low);
    }
}

/// A brightness reading. The first one is the starting value, not a change.
fn on_brightness(app: &AppHandle, level: u8) {
    let changed = {
        let power = app.state::<Power>();
        let Ok(mut s) = power.0.lock() else { return };
        s.snapshot.brightness.replace(level).is_some_and(|old| old != level)
    };
    if changed {
        let _ = app.emit("power://brightness", level);
    }
}

#[tauri::command]
pub fn power_get(power: State<'_, Power>) -> Result<PowerSnapshot, String> {
    Ok(power.0.lock().map_err(|e| e.to_string())?.snapshot)
}

/// Sets the brightness of the built-in screen (0–100).
#[tauri::command]
pub async fn power_set_brightness(level: u8) -> Result<(), String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || brightness::set(level.min(100)))
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        let _ = level;
        Err("not supported".into())
    }
}

#[cfg(windows)]
mod win {
    use super::{on_battery, on_brightness, BatteryState};
    use std::sync::OnceLock;
    use tauri::AppHandle;
    use windows::core::{w, GUID};
    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Power::{
        GetSystemPowerStatus, RegisterPowerSettingNotification, POWERBROADCAST_SETTING, SYSTEM_POWER_STATUS,
    };
    use windows::Win32::System::SystemServices::{
        GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING, GUID_POWER_SAVING_STATUS,
        GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
        DEVICE_NOTIFY_WINDOW_HANDLE, MSG, PBT_APMPOWERSTATUSCHANGE, PBT_POWERSETTINGCHANGE, WINDOW_EX_STYLE,
        WM_POWERBROADCAST, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// BatteryFlag: no system battery.
    const NO_BATTERY: u8 = 128;
    /// BatteryFlag: charging.
    const CHARGING: u8 = 8;
    /// BatteryFlag / percent / time: unknown.
    const UNKNOWN_FLAG: u8 = 255;
    const UNKNOWN_TIME: u32 = u32::MAX;

    fn read_battery() -> Option<BatteryState> {
        let mut s = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut s).ok()? };
        let present = s.BatteryFlag != UNKNOWN_FLAG && s.BatteryFlag & NO_BATTERY == 0 && s.BatteryLifePercent <= 100;
        let plugged = s.ACLineStatus == 1;
        Some(BatteryState {
            present,
            percent: if present { s.BatteryLifePercent } else { 0 },
            plugged,
            charging: present && plugged && s.BatteryFlag & CHARGING != 0,
            seconds_left: (present && !plugged && s.BatteryLifeTime != UNKNOWN_TIME).then_some(s.BatteryLifeTime),
            saver: s.SystemStatusFlag == 1,
        })
    }

    fn battery_changed() {
        if let (Some(app), Some(battery)) = (APP.get(), read_battery()) {
            on_battery(app, battery);
        }
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_POWERBROADCAST {
            match wparam.0 as u32 {
                PBT_POWERSETTINGCHANGE => {
                    let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
                    if setting.PowerSetting == GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS {
                        if setting.DataLength >= 4 && let Some(app) = APP.get() {
                            let level = unsafe { (setting.Data.as_ptr() as *const u32).read_unaligned() };
                            on_brightness(app, level.min(100) as u8);
                        }
                    } else {
                        battery_changed();
                    }
                }
                // Charging starting or stopping without a percent change.
                PBT_APMPOWERSTATUSCHANGE => battery_changed(),
                _ => {}
            }
            return LRESULT(1);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub fn spawn(app: AppHandle) {
        let _ = APP.set(app);
        std::thread::Builder::new()
            .name("duri-power".into())
            .spawn(|| unsafe {
                let instance = GetModuleHandleW(None).unwrap_or_default();
                let class = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance.into(),
                    lpszClassName: w!("DuriPowerListener"),
                    ..Default::default()
                };
                RegisterClassW(&class);
                // A hidden top-level window rather than a message-only one:
                // those don't get broadcasts such as PBT_APMPOWERSTATUSCHANGE.
                let Ok(hwnd) = CreateWindowExW(
                    WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
                    w!("DuriPowerListener"),
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                ) else {
                    log::error!("power listener window could not be created");
                    return;
                };
                let settings: [GUID; 4] = [
                    GUID_ACDC_POWER_SOURCE,
                    GUID_BATTERY_PERCENTAGE_REMAINING,
                    GUID_POWER_SAVING_STATUS,
                    GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS,
                ];
                for guid in &settings {
                    let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), guid, DEVICE_NOTIFY_WINDOW_HANDLE);
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("failed to spawn power thread");
    }
}

#[cfg(windows)]
mod brightness {
    use windows::core::{w, BSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE,
        RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Wmi::{
        IWbemClassObject, IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
        WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
    };

    const CLASS: &str = "WmiMonitorBrightnessMethods";

    pub fn set(level: u8) -> Result<(), String> {
        unsafe { run(level) }.map_err(|e| e.message().to_string())
    }

    unsafe fn run(level: u8) -> windows::core::Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let empty = BSTR::new();
            let services =
                locator.ConnectServer(&BSTR::from("ROOT\\WMI"), &empty, &empty, &empty, 0, &empty, None)?;
            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )?;
            // The instance path of the built-in screen.
            let found = services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(format!("SELECT * FROM {CLASS}")),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )?;
            let mut objects: [Option<IWbemClassObject>; 1] = [None];
            let mut count = 0u32;
            found.Next(WBEM_INFINITE, &mut objects, &mut count).ok()?;
            let Some(screen) = objects[0].take() else {
                return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
            };
            let mut path = VARIANT::default();
            screen.Get(w!("__PATH"), 0, &mut path, None, None)?;
            let path = BSTR::try_from(&path)?;

            let mut class = None;
            services.GetObject(&BSTR::from(CLASS), WBEM_GENERIC_FLAG_TYPE(0), None, Some(&mut class), None)?;
            let class = class.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            let mut signature = None;
            class.GetMethod(w!("WmiSetBrightness"), 0, &mut signature, std::ptr::null_mut())?;
            let signature =
                signature.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            let params = signature.SpawnInstance(0)?;
            // WMI passes uint32 as VT_I4.
            params.Put(w!("Timeout"), 0, &VARIANT::from(0i32), 0)?;
            params.Put(w!("Brightness"), 0, &VARIANT::from(level), 0)?;
            services.ExecMethod(
                &path,
                &BSTR::from("WmiSetBrightness"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                &params,
                None,
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_battery(percent: u8) -> BatteryState {
        BatteryState { present: true, percent, ..Default::default() }
    }

    #[test]
    fn warns_once_per_threshold() {
        let mut w = LowWatch::default();
        assert!(w.update(&on_battery(40)).is_none());
        assert!(w.update(&on_battery(21)).is_none());
        let low = w.update(&on_battery(20)).unwrap();
        assert_eq!((low.percent, low.critical), (20, false));
        assert!(w.update(&on_battery(15)).is_none());
        assert!(w.update(&on_battery(10)).unwrap().critical);
        assert!(w.update(&on_battery(5)).is_none());
    }

    #[test]
    fn plugging_in_resets_the_warnings() {
        let mut w = LowWatch::default();
        w.update(&on_battery(50));
        assert!(w.update(&on_battery(19)).is_some());
        assert!(w.update(&BatteryState { plugged: true, ..on_battery(19) }).is_none());
        assert!(w.update(&on_battery(19)).is_some());
    }

    #[test]
    fn starting_low_stays_quiet_until_the_next_threshold() {
        let mut w = LowWatch::default();
        assert!(w.update(&on_battery(15)).is_none());
        assert!(w.update(&on_battery(14)).is_none());
        assert!(w.update(&on_battery(9)).unwrap().critical);
    }

    #[test]
    fn desktops_never_warn() {
        let mut w = LowWatch::default();
        w.update(&BatteryState::default());
        assert!(w.update(&BatteryState { percent: 5, ..Default::default() }).is_none());
    }
}
