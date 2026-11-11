import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `BatteryState` in src-tauri/src/apps/power.rs. */
export interface BatteryState {
  present: boolean;
  /** 0–100 */
  percent: number;
  plugged: boolean;
  charging: boolean;
  secondsLeft: number | null;
  saver: boolean;
}

/** Mirrors `PowerSnapshot` in src-tauri/src/apps/power.rs. */
interface PowerSnapshot {
  battery: BatteryState;
  brightness: number | null;
}

/** Mirrors `LowBattery` in src-tauri/src/apps/power.rs. */
interface LowBattery {
  percent: number;
  critical: boolean;
}

export type BatteryNoticeKind = "plugged" | "unplugged" | "low" | "critical";

export interface BatteryNotice {
  kind: BatteryNoticeKind;
  seq: number;
}

interface PowerStore {
  battery: BatteryState;
  /** 0–100; null when the screen doesn't report it (desktop monitors). */
  brightness: number | null;
  notice: BatteryNotice | null;
  /** True for a moment after the brightness changes. */
  brightnessHud: boolean;
}

/** At or below these, the level turns red. Same as power.rs. */
export const LOW_BATTERY = 20;
export const CRITICAL_BATTERY = 10;

/** How long each notice stays. A critical battery also holds the island open. */
export const NOTICE_MS: Record<BatteryNoticeKind, number> = {
  plugged: 2500,
  unplugged: 2500,
  low: 5000,
  critical: 8000,
};
/** Same as the volume indicator. */
export const HUD_MS = 1600;

const store = createStore<PowerStore>({
  battery: { present: false, percent: 0, plugged: false, charging: false, secondsLeft: null, saver: false },
  brightness: null,
  notice: null,
  brightnessHud: false,
});
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
let hudTimer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;
let started = false;

function showNotice(kind: BatteryNoticeKind) {
  store.set((v) => ({ ...v, notice: { kind, seq: ++seq } }));
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => store.set((v) => ({ ...v, notice: null })), NOTICE_MS[kind]);
}

/** Shared by the Battery and Brightness apps; subscribes once. */
export function startPower() {
  if (started) return;
  started = true;
  invoke<PowerSnapshot>("power_get")
    .then((s) => store.set((v) => ({ ...v, battery: s.battery, brightness: s.brightness ?? v.brightness })))
    .catch(() => {});
  listen<BatteryState>("power://battery", (e) => store.set((v) => ({ ...v, battery: e.payload })));
  listen<boolean>("power://plugged", (e) => showNotice(e.payload ? "plugged" : "unplugged"));
  listen<LowBattery>("power://low", (e) => showNotice(e.payload.critical ? "critical" : "low"));
  listen<number>("power://brightness", (e) => {
    store.set((v) => ({ ...v, brightness: e.payload, brightnessHud: true }));
    clearTimeout(hudTimer);
    hudTimer = setTimeout(() => store.set((v) => ({ ...v, brightnessHud: false })), HUD_MS);
  });
}

export function usePower() {
  return useStore(store);
}

// Setting the brightness goes through WMI and takes a moment; while one call
// is running, only the latest level waits, so dragging the slider never
// queues up a backlog.
let sending = false;
let pending: number | null = null;

async function flush() {
  if (sending || pending === null) return;
  sending = true;
  const level = pending;
  pending = null;
  try {
    await invoke("power_set_brightness", { level });
  } catch {
    // The next change from Windows corrects the slider.
  } finally {
    sending = false;
    void flush();
  }
}

export function setBrightness(level: number) {
  const l = Math.round(Math.min(100, Math.max(0, level)));
  store.set((v) => ({ ...v, brightness: l }));
  pending = l;
  void flush();
}

/** "2 h 15 min" style time left, or null when Windows has no estimate. */
export function timeLeft(seconds: number | null): { h: number; m: number } | null {
  if (seconds === null || seconds <= 0) return null;
  const minutes = Math.round(seconds / 60);
  return { h: Math.floor(minutes / 60), m: minutes % 60 };
}
