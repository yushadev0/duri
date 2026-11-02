import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "./store";

export type LanguagePref = "system" | "en" | "tr";

export interface Settings {
  language: LanguagePref;
  disabledApps: string[];
}

export const settingsStore = createStore<Settings>({ language: "system", disabledApps: [] });

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
