import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { settingsStore, type Settings } from "../../lib/settings";
import { createStore, useStore } from "../../lib/store";

export type Device = "camera" | "microphone";

/** Mirrors `InUse` in src-tauri/src/apps/privacy.rs: app names per device. */
export type InUse = Record<Device, string[]>;

export interface PrivacyNotice {
  device: Device;
  /** The app that just started using it. */
  name: string;
  seq: number;
}

interface PrivacyStore {
  inUse: InUse;
  notice: PrivacyNotice | null;
}

/** Shown for a moment when an app starts using the camera or microphone. */
export const NOTICE_MS = 3000;

const store = createStore<PrivacyStore>({ inUse: { camera: [], microphone: [] }, notice: null });
let timer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;
let started = false;

/** Whether the person wants to see this device (Settings › Apps › Camera and Microphone). */
export function watched(device: Device, settings: Pick<Settings, "privacyCamera" | "privacyMicrophone">) {
  return device === "camera" ? settings.privacyCamera : settings.privacyMicrophone;
}

/** The first app that started using a watched device, camera first. */
export function newlyStarted(before: InUse, after: InUse, watch: (d: Device) => boolean): Omit<PrivacyNotice, "seq"> | null {
  for (const device of ["camera", "microphone"] as const) {
    if (!watch(device)) continue;
    const name = after[device].find((n) => !before[device].includes(n));
    if (name) return { device, name };
  }
  return null;
}

export function startPrivacy() {
  if (started) return;
  started = true;
  invoke<InUse>("privacy_get")
    .then((inUse) => store.set((v) => ({ ...v, inUse })))
    .catch(() => {});
  listen<InUse>("privacy://changed", (e) => {
    const settings = settingsStore.get();
    const started = newlyStarted(store.get().inUse, e.payload, (d) => watched(d, settings));
    store.set((v) => ({ ...v, inUse: e.payload }));
    if (!started) return;
    store.set((v) => ({ ...v, notice: { ...started, seq: ++seq } }));
    clearTimeout(timer);
    timer = setTimeout(() => store.set((v) => ({ ...v, notice: null })), NOTICE_MS);
  });
}

export function usePrivacy() {
  return useStore(store);
}
