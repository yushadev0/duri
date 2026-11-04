import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `DeviceKind` in src-tauri/src/apps/bluetooth.rs. */
export type DeviceKind = "headphones" | "speaker" | "phone" | "computer" | "keyboard" | "mouse" | "gamepad" | "other";

/** Mirrors `DeviceEvent` in src-tauri/src/apps/bluetooth.rs. */
export interface DeviceEvent {
  name: string;
  connected: boolean;
  kind: DeviceKind;
}

interface BluetoothStore {
  /** The device that just connected or disconnected, shown for a moment. */
  notice: (DeviceEvent & { seq: number }) | null;
  /** Newest first, this session only. */
  recent: (DeviceEvent & { at: number })[];
}

/** Same as a Windows notification in the island. */
export const NOTICE_MS = 4000;
const RECENT = 6;

const store = createStore<BluetoothStore>({ notice: null, recent: [] });
let timer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;
let started = false;

export function startBluetooth() {
  if (started) return;
  started = true;
  listen<DeviceEvent>("bluetooth://device", (e) => {
    const event = e.payload;
    store.set((v) => ({
      notice: { ...event, seq: ++seq },
      recent: [{ ...event, at: Date.now() }, ...v.recent].slice(0, RECENT),
    }));
    clearTimeout(timer);
    timer = setTimeout(() => store.set((v) => ({ ...v, notice: null })), NOTICE_MS);
  });
}

export function useBluetooth() {
  return useStore(store);
}
