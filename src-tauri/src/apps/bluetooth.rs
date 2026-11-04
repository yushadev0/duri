//! Bluetooth app: a short notice in the island when a paired Bluetooth device
//! connects or disconnects.
//!
//! A WinRT `DeviceWatcher` over paired Bluetooth association endpoints (both
//! classic and LE) reports `System.Devices.Aep.IsConnected` changes as they
//! happen, so nothing is polled. The first enumeration only records the
//! current state; after that, every change emits `bluetooth://device`.

use serde::Serialize;
#[cfg(windows)]
use std::time::Duration;
use tauri::AppHandle;

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
    pub name: String,
    pub connected: bool,
    pub kind: DeviceKind,
}

pub fn init(app: &AppHandle) {
    #[cfg(windows)]
    {
        let app = app.clone();
        std::thread::Builder::new()
            .name("duri-bluetooth".into())
            .spawn(move || watch::run(app))
            .expect("failed to spawn bluetooth thread");
    }
    #[cfg(not(windows))]
    let _ = app;
}

/// A dual-mode device shows up as a classic and an LE endpoint that change
/// together; one notice is enough.
#[cfg(windows)]
const SAME_NOTICE: Duration = Duration::from_secs(4);

#[cfg(windows)]
mod watch {
    use super::{kind_of, DeviceEvent, DeviceKind, SAME_NOTICE};
    use std::collections::HashMap;
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

    struct Device {
        name: String,
        connected: bool,
        kind: DeviceKind,
        /// Shared by every endpoint of one physical device.
        container: String,
    }

    #[derive(Default)]
    struct Known {
        devices: HashMap<String, Device>,
        /// The first enumeration is the current state, not news.
        ready: bool,
        /// Last notice per physical device, to drop the twin endpoint's.
        sent: HashMap<String, (bool, Instant)>,
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

    fn prop_string(props: &Props, key: &str) -> Option<String> {
        let v = props.Lookup(&HSTRING::from(key)).ok()?;
        if let Ok(r) = v.cast::<IReference<windows::core::GUID>>() {
            return r.Value().ok().map(|g| format!("{g:?}"));
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
        let event = DeviceEvent { name: d.name.clone(), connected: d.connected, kind: d.kind };
        let _ = app.emit("bluetooth://device", event);
    }

    fn create(app: &AppHandle, known: &Arc<Mutex<Known>>) -> windows::core::Result<DeviceWatcher> {
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

        let (k, a) = (known.clone(), app.clone());
        watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(move |_, info| {
            let Some(info) = info.as_ref() else { return Ok(()) };
            let props = info.Properties()?;
            let id = info.Id()?.to_string();
            let device = Device {
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
                }
            }
            Ok(())
        }))?;

        let (k, a) = (known.clone(), app.clone());
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
            }
            Ok(())
        }))?;

        let k = known.clone();
        watcher.Removed(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(move |_, update| {
            if let Some(update) = update.as_ref()
                && let Ok(mut known) = k.lock()
            {
                known.devices.remove(&update.Id()?.to_string());
            }
            Ok(())
        }))?;

        let k = known.clone();
        watcher.EnumerationCompleted(&TypedEventHandler::new(move |_, _| {
            if let Ok(mut known) = k.lock() {
                known.ready = true;
            }
            Ok(())
        }))?;

        watcher.Start()?;
        Ok(watcher)
    }

    /// Keeps a watcher running, and starts a new one if Windows stops it
    /// (the Bluetooth service restarting, for example).
    pub fn run(app: AppHandle) {
        loop {
            let known = Arc::new(Mutex::new(Known::default()));
            let watcher = match create(&app, &known) {
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
}
