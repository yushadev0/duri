/**
 * Shared store for the Timer and Stopwatch apps. Both live in one backend
 * module (src-tauri/src/apps/timers.rs) and arrive as one snapshot.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import i18n from "../../i18n";
import { formatDurationShort } from "../../lib/format";
import { playChime } from "../../lib/sound";
import { createStore, useStore } from "../../lib/store";

export type TimerState =
  | { state: "running"; endsAt: number }
  | { state: "paused"; remainingMs: number }
  | { state: "finished"; finishedAt: number };

export type Timer = {
  id: string;
  label: string | null;
  durationMs: number;
  createdAt: number;
} & TimerState;

export interface Stopwatch {
  runningSince: number | null;
  accumulatedMs: number;
  /** Cumulative elapsed time at each lap press. */
  laps: number[];
}

export interface Snapshot {
  timers: Timer[];
  stopwatch: Stopwatch;
}

const store = createStore<Snapshot>({
  timers: [],
  stopwatch: { runningSince: null, accumulatedMs: 0, laps: [] },
});

export function useTimersSnapshot() {
  return useStore(store);
}

export function durationLabel(ms: number): string {
  return formatDurationShort(ms, {
    h: i18n.t("timer.units.h"),
    min: i18n.t("timer.units.min"),
    s: i18n.t("timer.units.s"),
  });
}

async function notifyFinished(timer: Timer) {
  playChime();
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (!granted) return;
    sendNotification({
      title: timer.label ?? i18n.t("timer.done"),
      body: i18n.t("timer.notifyBody", { duration: durationLabel(timer.durationMs) }),
    });
  } catch {
    // The island alert is the primary signal; notifications are best effort.
  }
}

let started = false;

export function startTimers() {
  if (started) return;
  started = true;
  invoke<Snapshot>("timers_get").then((s) => store.set(s));
  listen<Snapshot>("timers://changed", (e) => store.set(e.payload));
  listen<Timer>("timers://finished", (e) => notifyFinished(e.payload));
}

export function remainingMs(t: Timer, now: number): number {
  switch (t.state) {
    case "running":
      return Math.max(0, t.endsAt - now);
    case "paused":
      return t.remainingMs;
    case "finished":
      return 0;
  }
}

/** The timer the island should talk about: finished first, then soonest to end. */
export function primaryTimer(timers: Timer[]): Timer | undefined {
  const finished = timers.filter((t) => t.state === "finished");
  if (finished.length) return finished[0];
  const running = timers
    .filter((t): t is Timer & { state: "running"; endsAt: number } => t.state === "running")
    .sort((a, b) => a.endsAt - b.endsAt);
  return running[0] ?? timers[0];
}

export function stopwatchElapsed(w: Stopwatch, now: number): number {
  return w.accumulatedMs + (w.runningSince !== null ? Math.max(0, now - w.runningSince) : 0);
}

export const timerActions = {
  start: (durationMs: number, label?: string) => invoke<string>("timer_start", { durationMs, label: label ?? null }),
  pause: (id: string) => invoke("timer_pause", { id }),
  resume: (id: string) => invoke("timer_resume", { id }),
  restart: (id: string) => invoke("timer_restart", { id }),
  remove: (id: string) => invoke("timer_remove", { id }),
};

export const stopwatchActions = {
  start: () => invoke("stopwatch_start"),
  pause: () => invoke("stopwatch_pause"),
  lap: () => invoke("stopwatch_lap"),
  reset: () => invoke("stopwatch_reset"),
};
