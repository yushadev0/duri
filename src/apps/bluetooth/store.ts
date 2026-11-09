import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `DeviceKind` in src-tauri/src/apps/bluetooth.rs. */
export type DeviceKind = "headphones" | "speaker" | "phone" | "computer" | "keyboard" | "mouse" | "gamepad" | "other";

/** Mirrors `DeviceEvent` in src-tauri/src/apps/bluetooth.rs. */
export interface DeviceEvent {
  /** Same as `ConnectedDevice.id` for the same device. */
  id: string;
  name: string;
  connected: boolean;
  kind: DeviceKind;
}

/** Mirrors `ConnectedDevice` in src-tauri/src/apps/bluetooth.rs. */
export interface ConnectedDevice {
  id: string;
  name: string;
  kind: DeviceKind;
  /** 0–100, null when the device doesn't report it. */
  battery: number | null;
}

/** Mirrors `LowBattery` in src-tauri/src/apps/bluetooth.rs. */
interface LowBattery {
  id: string;
  name: string;
  kind: DeviceKind;
  battery: number;
}

/** A connection change, or (with `low`) a battery that just became low. */
export type Notice = DeviceEvent & { seq: number; low?: number };

interface BluetoothStore {
  /** Shown for a moment. */
  notice: Notice | null;
  /** Connected devices, sorted by name. */
  devices: ConnectedDevice[];
}

/** Same as a Windows notification in the island. */
export const NOTICE_MS = 4000;
/** At or below this, the level turns red. Same as `LOW_BATTERY` in bluetooth.rs. */
export const LOW_BATTERY = 20;

const store = createStore<BluetoothStore>({ notice: null, devices: [] });
let timer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;
let started = false;

export function startBluetooth() {
  if (started) return;
  started = true;
  const show = (notice: Notice) => {
    store.set((v) => ({ ...v, notice }));
    clearTimeout(timer);
    timer = setTimeout(() => store.set((v) => ({ ...v, notice: null })), NOTICE_MS);
  };
  listen<DeviceEvent>("bluetooth://device", (e) => show({ ...e.payload, seq: ++seq }));
  listen<LowBattery>("bluetooth://low", (e) => {
    const { id, name, kind, battery } = e.payload;
    show({ id, name, kind, connected: true, low: battery, seq: ++seq });
  });
  listen<ConnectedDevice[]>("bluetooth://devices", (e) => store.set((v) => ({ ...v, devices: e.payload })));
  invoke<ConnectedDevice[]>("bluetooth_devices")
    .then((devices) => store.set((v) => ({ ...v, devices })))
    .catch(() => {});
}

export function useBluetooth() {
  return useStore(store);
}

/** The battery of a device that is still connected, for the notice. */
export function batteryOf(devices: ConnectedDevice[], id: string): number | null {
  return devices.find((d) => d.id === id)?.battery ?? null;
}
