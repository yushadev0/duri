import { invoke } from "@tauri-apps/api/core";
import { createStore, useStore } from "./store";
import { useSettings, type ClockFormat } from "./settings";

/** Whether Windows uses a 24-hour clock; asked once at startup. */
const system24h = createStore<boolean>(true);
let asked = false;

function askSystem() {
  if (asked) return;
  asked = true;
  invoke<boolean>("system_clock_24h")
    .then((v) => system24h.set(v))
    .catch(() => {});
}

export function resolveHour12(format: ClockFormat, systemIs24h: boolean): boolean {
  if (format === "12h") return true;
  if (format === "24h") return false;
  return !systemIs24h;
}

/** Whether clocks should show 12-hour time (Settings › General › Clock). */
export function useHour12(): boolean {
  askSystem();
  const { clockFormat } = useSettings();
  const is24h = useStore(system24h);
  return resolveHour12(clockFormat, is24h);
}

/** "15:04" or "3:04 PM", in the UI language's style. */
export function formatClock(date: Date, language: string, hour12: boolean): string {
  return date.toLocaleTimeString(
    language,
    hour12 ? { hour: "numeric", minute: "2-digit", hourCycle: "h12" } : { hour: "2-digit", minute: "2-digit", hourCycle: "h23" },
  );
}

/**
 * Weekday and day for the idle pill: "Tue 3" in English, "3 Sal" in Turkish.
 * English is formatted as en-GB, because en-US puts the day first ("3 Tue").
 */
export function formatShortDate(date: Date, language: string): string {
  const locale = language.startsWith("en") ? "en-GB" : language;
  return date.toLocaleDateString(locale, { weekday: "short", day: "numeric" });
}
