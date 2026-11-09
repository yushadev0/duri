//! Bluetooth app: the connected devices with their battery level, and a short
//! notice in the island when a paired device connects or disconnects.
//!
//! A WinRT `DeviceWatcher` over paired Bluetooth association endpoints (both
//! classic and LE) reports `System.Devices.Aep.IsConnected` changes as they
//! happen. The first enumeration only records the current state; after that,
//! every change emits `bluetooth://device`.
//!
//! Battery levels come from the devices' PnP nodes (the Hands-Free node of
//! headphones, the HID node of LE mice and keyboards), which share the
//! endpoint's container id. Windows doesn't report changes to them, so they
//! are read every minute, and every few seconds right after a connection
//! (headphones report their level a moment after they connect). The connected
//! list goes out as `bluetooth://devices` whenever it changes.

use serde::Serialize;
use std::sync::Mutex;
#[cfg(windows)]
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// What kind of device it is, from the Bluetooth class of device. Picks the
/// icon in the island.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Headphones,
    Speaker,
    Phone,
    Computer,
    Keyboard,
    Mouse,
    Gamepad,
    Other,
}

/// Maps a class of device (major, minor as Windows reports them: the minor
/// is the 6-bit field, bits 2–7 of the CoD) to a kind. LE devices have no
/// class of device and end up as `Other`.
pub fn kind_of(major: Option<u32>, minor: Option<u32>) -> DeviceKind {
    let minor = minor.unwrap_or(0);
    match major {
        Some(1) => DeviceKind::Computer,
        Some(2) => DeviceKind::Phone,
        // 5 = loudspeaker, 7 = portable audio, 10 = hi-fi audio.
        Some(4) if matches!(minor, 5 | 7 | 10) => DeviceKind::Speaker,
        Some(4) => DeviceKind::Headphones,
        Some(5) => match (minor >> 4, minor & 0x0f) {
            (_, 1 | 2) => DeviceKind::Gamepad,
            (1, _) => DeviceKind::Keyboard,
            (2, _) => DeviceKind::Mouse,
            (3, _) => DeviceKind::Keyboard,
            _ => DeviceKind::Other,
        },
        _ => DeviceKind::Other,
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEvent {
    /// The physical device (container id), same as `ConnectedDevice::id`.
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub kind: DeviceKind,
}

/// A connected device, one per physical device.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDevice {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    /// 0–100, `None` when the device doesn't report it.
    pub battery: Option<u8>,
}

/// One Bluetooth endpoint as the watcher sees it. A dual-mode device has two
/// (classic and LE) with the same `container`.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub name: String,
    pub connected: bool,
    pub kind: DeviceKind,
    pub container: String,
}

/// Groups endpoints into connected physical devices, sorted by name. The kind
/// comes from whichever endpoint has a class of device.
pub fn connected_devices<'a>(
    endpoints: impl IntoIterator<Item = &'a Endpoint>,
    battery: impl Fn(&str) -> Option<u8>,
) -> Vec<ConnectedDevice> {
    let mut out: Vec<ConnectedDevice> = Vec::new();
    for e in endpoints {
        if !e.connected {
            continue;
        }
        match out.iter_mut().find(|d| d.id == e.container) {
            Some(d) => {
                if d.name.is_empty() {
                    d.name = e.name.clone();
                }
                if d.kind == DeviceKind::Other {
                    d.kind = e.kind;
                }
            }
            None => out.push(ConnectedDevice {
                id: e.container.clone(),
                name: e.name.clone(),
                kind: e.kind,
                battery: battery(&e.container),
            }),
        }
    }
    out.retain(|d| !d.name.is_empty());
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// At or below this level a device's battery is low (Windows warns at 20%).
pub const LOW_BATTERY: u8 = 20;

/// `bluetooth://low`: a connected device's battery just became low.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LowBattery {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub battery: u8,
}

/// Whether a new battery reading is worth a low battery notice: once when the
/// level drops to `LOW_BATTERY` or below, or when a device that just connected
/// reports a low level. Devices that were already low when Duri started stay
/// quiet (the list shows them in red).
pub fn is_low_news(previous: Option<u8>, now: u8, just_connected: bool) -> bool {
    now <= LOW_BATTERY
        && match previous {
            Some(p) => p > LOW_BATTERY,
            None => just_connected,
        }
}

/// The last list sent to the UI, for `bluetooth_devices`.
#[derive(Default)]
pub struct Devices(Mutex<Vec<ConnectedDevice>>);

#[tauri::command]
pub fn bluetooth_devices(devices: State<'_, Devices>) -> Vec<ConnectedDevice> {
    devices.0.lock().map(|d| d.clone()).unwrap_or_default()
}

/// Sends the list if it changed.
#[cfg_attr(not(windows), allow(dead_code))]
fn publish(app: &AppHandle, list: Vec<ConnectedDevice>) {
    use tauri::Emitter;
    let state = app.state::<Devices>();
    let Ok(mut last) = state.0.lock() else { return };
    if *last != list {
        *last = list.clone();
        drop(last);
        let _ = app.emit("bluetooth://devices", list);
    }
}

pub fn init(app: &AppHandle) {
    app.manage(Devices::default());
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::Builder::new()
            .name("duri-bluetooth".into())
            .spawn(move || watch::run(app))
            .expect("failed to spawn bluetooth thread");
    }
}

/// A dual-mode device shows up as a classic and an LE endpoint that change
/// together; one notice is enough.
#[cfg(windows)]
const SAME_NOTICE: Duration = Duration::from_secs(4);

#[cfg(windows)]
mod watch {
    use super::{
        connected_devices, is_low_news, kind_of, publish, ConnectedDevice, DeviceEvent, Endpoint, LowBattery,
        SAME_NOTICE,
    };
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};
    use windows::core::{Interface, HSTRING};
    use windows::Devices::Enumeration::{
        DeviceInformation, DeviceInformationKind, DeviceInformationUpdate, DeviceWatcher, DeviceWatcherStatus,
    };
    use windows::Foundation::{IReference, TypedEventHandler};
    use windows_collections::{IIterable, IMapView};

    const CLASSIC: &str = "{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}";
    const LE: &str = "{bb7bb05e-5972-42b5-94fc-76eaa7084d49}";
    const CONNECTED: &str = "System.Devices.Aep.IsConnected";
    const CONTAINER: &str = "System.Devices.Aep.ContainerId";
    const MAJOR: &str = "System.Devices.Aep.Bluetooth.Cod.Major";
    const MINOR: &str = "System.Devices.Aep.Bluetooth.Cod.Minor";
    /// DEVPKEY_Bluetooth_Battery: the level Windows shows in Settings.
    const BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";
    const DEVICE_CONTAINER: &str = "System.Devices.ContainerId";

    /// Battery read interval while nothing changes.
    const SLOW: Duration = Duration::from_secs(60);
    /// Read interval for a while after a connection change.
    const FAST: Duration = Duration::from_secs(4);
    const FAST_FOR: Duration = Duration::from_secs(40);
    /// A first low reading this soon after connecting still gets a notice.
    const JUST_CONNECTED: Duration = Duration::from_secs(120);

    #[derive(Default)]
    struct Known {
        devices: HashMap<String, Endpoint>,
        /// The first enumeration is the current state, not news.
        ready: bool,
        /// Last notice per physical device, to drop the twin endpoint's.
        sent: HashMap<String, (bool, Instant)>,
        /// Battery level per container.
        battery: HashMap<String, u8>,
    }

    impl Known {
        fn list(&self) -> Vec<ConnectedDevice> {
            connected_devices(self.devices.values(), |c| self.battery.get(c).copied())
        }

        fn publish(&self, app: &AppHandle) {
            if self.ready {
                publish(app, self.list());
            }
        }
    }

    type Props = IMapView<HSTRING, windows::core::IInspectable>;

    fn prop_bool(props: &Props, key: &str) -> Option<bool> {
        let v = props.Lookup(&HSTRING::from(key)).ok()?;
        v.cast::<IReference<bool>>().ok()?.Value().ok()
    }

    fn prop_u32(props: &Props, key: &str) -> Option<u32> {
        let v = props.Lookup(&HSTRING::from(key)).ok()?;
        if let Ok(r) = v.cast::<IReference<u32>>() {
            return r.Value().ok();
        }
        if let Ok(r) = v.cast::<IReference<u8>>() {
            return r.Value().ok().map(u32::from);
        }
        v.cast::<IReference<u16>>().ok()?.Value().ok().map(u32::from)
    }

    /// GUIDs come out as `{XXXXXXXX-…}`, the form AQS filters take.
    fn prop_string(props: &Props, key: &str) -> Option<String> {
        let v = props.Lookup(&HSTRING::from(key)).ok()?;
        if let Ok(r) = v.cast::<IReference<windows::core::GUID>>() {
            return r.Value().ok().map(|g| format!("{{{g:?}}}"));
        }
        v.cast::<IReference<HSTRING>>().ok()?.Value().ok().map(|h| h.to_string())
    }

    fn notify(app: &AppHandle, known: &mut Known, id: &str) {
        let Some(d) = known.devices.get(id) else { return };
        if !known.ready || d.name.is_empty() {
            return;
        }
        let now = Instant::now();
        if let Some(&(connected, at)) = known.sent.get(&d.container)
            && connected == d.connected
            && now.duration_since(at) < SAME_NOTICE
        {
            return;
        }
        known.sent.insert(d.container.clone(), (d.connected, now));
        let event =
            DeviceEvent { id: d.container.clone(), name: d.name.clone(), connected: d.connected, kind: d.kind };
        let _ = app.emit("bluetooth://device", event);
    }

    fn create(app: &AppHandle, known: &Arc<Mutex<Known>>, wake: &Sender<()>) -> windows::core::Result<DeviceWatcher> {
        let aqs = format!(
            "(System.Devices.Aep.ProtocolId:=\"{CLASSIC}\" OR System.Devices.Aep.ProtocolId:=\"{LE}\") \
             AND System.Devices.Aep.IsPaired:=System.StructuredQueryType.Boolean#True"
        );
        let props: IIterable<HSTRING> =
            vec![HSTRING::from(CONNECTED), HSTRING::from(CONTAINER), HSTRING::from(MAJOR), HSTRING::from(MINOR)].into();
        let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
            &HSTRING::from(aqs),
            &props,
            DeviceInformationKind::AssociationEndpoint,
        )?;

        let (k, a, w) = (known.clone(), app.clone(), wake.clone());
        watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(move |_, info| {
            let Some(info) = info.as_ref() else { return Ok(()) };
            let props = info.Properties()?;
            let id = info.Id()?.to_string();
            let device = Endpoint {
                name: info.Name()?.to_string(),
                connected: prop_bool(&props, CONNECTED).unwrap_or(false),
                kind: kind_of(prop_u32(&props, MAJOR), prop_u32(&props, MINOR)),
                container: prop_string(&props, CONTAINER).unwrap_or_else(|| id.clone()),
            };
            let connected = device.connected;
            if let Ok(mut known) = k.lock() {
                known.devices.insert(id.clone(), device);
                // Newly paired and already connected.
                if connected {
                    notify(&a, &mut known, &id);
                    known.publish(&a);
                }
            }
            if connected {
                let _ = w.send(());
            }
            Ok(())
        }))?;

        let (k, a, w) = (known.clone(), app.clone(), wake.clone());
        watcher.Updated(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(move |_, update| {
            let Some(update) = update.as_ref() else { return Ok(()) };
            let Some(connected) = prop_bool(&update.Properties()?, CONNECTED) else { return Ok(()) };
            let id = update.Id()?.to_string();
            if let Ok(mut known) = k.lock()
                && let Some(d) = known.devices.get_mut(&id)
                && d.connected != connected
            {
                d.connected = connected;
                notify(&a, &mut known, &id);
                known.publish(&a);
                let _ = w.send(());
            }
            Ok(())
        }))?;

        let (k, a) = (known.clone(), app.clone());
        watcher.Removed(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(move |_, update| {
            if let Some(update) = update.as_ref()
                && let Ok(mut known) = k.lock()
            {
                known.devices.remove(&update.Id()?.to_string());
                known.publish(&a);
            }
            Ok(())
        }))?;

        let (k, a, w) = (known.clone(), app.clone(), wake.clone());
        watcher.EnumerationCompleted(&TypedEventHandler::new(move |_, _| {
            if let Ok(mut known) = k.lock() {
                known.ready = true;
                known.publish(&a);
            }
            let _ = w.send(());
            Ok(())
        }))?;

        watcher.Start()?;
        Ok(watcher)
    }

    /// Reads the battery level of the given containers. Several PnP nodes of
    /// one device may report it; they agree, so the first one wins.
    fn read_batteries(containers: &[String]) -> HashMap<String, u8> {
        let mut out = HashMap::new();
        if containers.is_empty() {
            return out;
        }
        let filter =
            containers.iter().map(|c| format!("{DEVICE_CONTAINER}:=\"{c}\"")).collect::<Vec<_>>().join(" OR ");
        let props: IIterable<HSTRING> = vec![HSTRING::from(BATTERY), HSTRING::from(DEVICE_CONTAINER)].into();
        let found = DeviceInformation::FindAllAsyncWithKindAqsFilterAndAdditionalProperties(
            &HSTRING::from(filter),
            &props,
            DeviceInformationKind::Device,
        )
        .and_then(|op| op.join());
        let found = match found {
            Ok(f) => f,
            Err(e) => {
                log::warn!("bluetooth battery query failed: {e}");
                return out;
            }
        };
        for info in found {
            let Ok(props) = info.Properties() else { continue };
            let (Some(level), Some(container)) = (prop_u32(&props, BATTERY), prop_string(&props, DEVICE_CONTAINER))
            else {
                continue;
            };
            // The two APIs may spell the GUID in a different case.
            if let Some(c) = containers.iter().find(|c| c.eq_ignore_ascii_case(&container)) {
                out.entry(c.clone()).or_insert(level.min(100) as u8);
            }
        }
        out
    }

    /// Reads battery levels every `SLOW`, and every `FAST` for `FAST_FOR`
    /// after a connection change.
    fn poll_batteries(app: AppHandle, known: Arc<Mutex<Known>>, wake: Receiver<()>) {
        let mut fast_until = Instant::now();
        loop {
            let wait = if Instant::now() < fast_until { FAST } else { SLOW };
            match wake.recv_timeout(wait) {
                Ok(()) => {
                    fast_until = Instant::now() + FAST_FOR;
                    // Let a burst of changes (both endpoints of a device) settle.
                    std::thread::sleep(Duration::from_millis(1500));
                    while wake.try_recv().is_ok() {}
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            let containers: Vec<String> = match known.lock() {
                Ok(k) => k.list().into_iter().map(|d| d.id).collect(),
                Err(_) => return,
            };
            let levels = read_batteries(&containers);
            if let Ok(mut k) = known.lock() {
                if k.ready {
                    for device in k.list() {
                        let Some(&now) = levels.get(&device.id) else { continue };
                        let just_connected = k
                            .sent
                            .get(&device.id)
                            .is_some_and(|&(connected, at)| connected && at.elapsed() < JUST_CONNECTED);
                        if is_low_news(k.battery.get(&device.id).copied(), now, just_connected) {
                            let low = LowBattery { id: device.id, name: device.name, kind: device.kind, battery: now };
                            let _ = app.emit("bluetooth://low", low);
                        }
                    }
                }
                // A connected device that stops reporting keeps its last
                // level; disconnected devices lose theirs.
                k.battery.retain(|c, _| containers.contains(c));
                k.battery.extend(levels);
                k.publish(&app);
            }
        }
    }

    /// Keeps a watcher running, and starts a new one if Windows stops it
    /// (the Bluetooth service restarting, for example).
    pub fn run(app: AppHandle) {
        let known = Arc::new(Mutex::new(Known::default()));
        let (wake, rx) = mpsc::channel();
        {
            let (app, known) = (app.clone(), known.clone());
            std::thread::Builder::new()
                .name("duri-bluetooth-battery".into())
                .spawn(move || poll_batteries(app, known, rx))
                .expect("failed to spawn bluetooth battery thread");
        }
        loop {
            if let Ok(mut k) = known.lock() {
                *k = Known::default();
            }
            let watcher = match create(&app, &known, &wake) {
                Ok(w) => w,
                Err(e) => {
                    log::warn!("bluetooth watcher could not start: {e}");
                    std::thread::sleep(Duration::from_secs(60));
                    continue;
                }
            };
            loop {
                std::thread::sleep(Duration::from_secs(10));
                match watcher.Status() {
                    Ok(DeviceWatcherStatus::Started | DeviceWatcherStatus::EnumerationCompleted) => {}
                    _ => break,
                }
            }
            let _ = watcher.Stop();
            publish(&app, Vec::new());
            std::thread::sleep(Duration::from_secs(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_class_of_device() {
        // Measured: Philips TAH4205 and JBL Wave Beam 2 (4, 1), DualSense (5, 2).
        assert_eq!(kind_of(Some(4), Some(1)), DeviceKind::Headphones);
        assert_eq!(kind_of(Some(4), Some(5)), DeviceKind::Speaker);
        assert_eq!(kind_of(Some(5), Some(2)), DeviceKind::Gamepad);
        assert_eq!(kind_of(Some(5), Some(0x10)), DeviceKind::Keyboard);
        assert_eq!(kind_of(Some(5), Some(0x20)), DeviceKind::Mouse);
        assert_eq!(kind_of(Some(2), Some(3)), DeviceKind::Phone);
        assert_eq!(kind_of(None, None), DeviceKind::Other);
    }

    #[test]
    fn low_battery_news_once() {
        // Dropping to the threshold or below, once.
        assert!(is_low_news(Some(21), 20, false));
        assert!(is_low_news(Some(40), 12, false));
        assert!(!is_low_news(Some(20), 19, false));
        assert!(!is_low_news(Some(30), 25, false));
        // Connected just now at a low level.
        assert!(is_low_news(None, 10, true));
        // Already low when Duri started.
        assert!(!is_low_news(None, 10, false));
    }

    fn ep(name: &str, connected: bool, kind: DeviceKind, container: &str) -> Endpoint {
        Endpoint { name: name.into(), connected, kind, container: container.into() }
    }

    #[test]
    fn groups_endpoints_into_connected_devices() {
        let endpoints = [
            // Dual-mode headphones: the LE endpoint has no class of device.
            ep("WH-1000XM5", true, DeviceKind::Other, "{A}"),
            ep("WH-1000XM5", true, DeviceKind::Headphones, "{A}"),
            ep("keyboard", false, DeviceKind::Keyboard, "{B}"),
            ep("DualSense", true, DeviceKind::Gamepad, "{C}"),
            ep("", true, DeviceKind::Other, "{D}"),
        ];
        let list = connected_devices(&endpoints, |c| (c == "{A}").then_some(80));
        assert_eq!(
            list,
            vec![
                ConnectedDevice { id: "{C}".into(), name: "DualSense".into(), kind: DeviceKind::Gamepad, battery: None },
                ConnectedDevice {
                    id: "{A}".into(),
                    name: "WH-1000XM5".into(),
                    kind: DeviceKind::Headphones,
                    battery: Some(80),
                },
            ]
        );
    }
}

