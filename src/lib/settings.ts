import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "./store";

export type LanguagePref = "system" | "en" | "tr";

export type IslandStyle = "floating" | "attached";
export type PillSize = "compact" | "default" | "large";
export type IdleInfo = "none" | "clock" | "stats" | "both";
export type ClockFormat = "system" | "12h" | "24h";
export type Transparency = "off" | "low" | "high";

export interface Settings {
  language: LanguagePref;
  disabledApps: string[];
  islandStyle: IslandStyle;
  pillSize: PillSize;
  replaceVolumeFlyout: boolean;
  idleInfo: IdleInfo;
  clockFormat: ClockFormat;
  transparency: Transparency;
  hideInFullscreen: boolean;
  autoUpdateCheck: boolean;
}

export const settingsStore = createStore<Settings>({
  language: "system",
  disabledApps: [],
  islandStyle: "floating",
  pillSize: "default",
  replaceVolumeFlyout: false,
  idleInfo: "clock",
  clockFormat: "system",
  transparency: "off",
  hideInFullscreen: true,
  autoUpdateCheck: true,
});

let started = false;

/** Loads settings once and keeps every window in sync with changes. */
export function startSettingsSync() {
  if (started) return;
  started = true;
  invoke<Settings>("settings_get").then((s) => settingsStore.set(s));
  listen<Settings>("settings://changed", (e) => settingsStore.set(e.payload));
}

export async function updateSettings(patch: Partial<Settings>) {
  const next = { ...settingsStore.get(), ...patch };
  settingsStore.set(next);
  await invoke("settings_set", { settings: next });
}

export function useSettings() {
  return useStore(settingsStore);
}
