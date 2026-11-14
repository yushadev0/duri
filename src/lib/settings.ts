import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "./store";

export type LanguagePref = "system" | "en" | "tr";

export type IslandStyle = "floating" | "attached";
export type PillSize = "compact" | "default" | "large";
export type IdleInfo = "none" | "clock" | "stats" | "both";
export type ClockFormat = "system" | "12h" | "24h";
export type Transparency = "off" | "low" | "high";
export type ClockAlign = "left" | "center" | "right";
export type MusicIdleStyle = "auto" | "network" | "reactive" | "lyrics" | "cover";

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
  tucked: boolean;
  autoHide: boolean;
  shortcutIsland: string;
  shortcutNote: string;
  shortcutClipboard: string;
  /** Privacy app: show apps using the camera / the microphone. */
  privacyCamera: boolean;
  privacyMicrophone: boolean;
  /** Replace the text of the Windows tray clock (loads a helper into explorer). */
  taskbarClock: boolean;
  taskbarClockLine1: string;
  taskbarClockLine2: string;
  taskbarClockLine3: string;
  taskbarClockAlign: ClockAlign;
  /** The city the weather tokens are for; nothing is fetched until one is picked. */
  taskbarClockCity: string;
  taskbarClockLatitude: number | null;
  taskbarClockLongitude: number | null;
  taskbarClockFahrenheit: boolean;
  /** Music › Idle Mode look; "auto" is Network, or Lyrics under reduced motion. */
  musicIdleStyle: MusicIdleStyle;
  /** Music: the line being sung shows in the middle of the pill (songs with timed lyrics). */
  musicPillLyrics: boolean;
  /** Music: the time beside the lyrics in the pill. */
  musicPillClock: boolean;
  /** Music › Idle Mode: a clock at the top of the screen. */
  musicIdleClock: boolean;
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
  tucked: false,
  autoHide: false,
  shortcutIsland: "Ctrl+Alt+Space",
  shortcutNote: "",
  shortcutClipboard: "",
  privacyCamera: true,
  privacyMicrophone: true,
  taskbarClock: false,
  taskbarClockLine1: "{time}",
  taskbarClockLine2: "CPU {cpu}%  RAM {ram}%",
  taskbarClockLine3: "",
  taskbarClockAlign: "right",
  taskbarClockCity: "",
  taskbarClockLatitude: null,
  taskbarClockLongitude: null,
  taskbarClockFahrenheit: false,
  musicIdleStyle: "auto",
  musicPillLyrics: false,
  musicPillClock: false,
  musicIdleClock: true,
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
