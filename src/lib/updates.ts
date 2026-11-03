import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import i18n from "../i18n";
import { settingsStore } from "./settings";
import { createStore, useStore } from "./store";

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; version: string; progress: number | null }
  | { kind: "error"; message: string };

const store = createStore<UpdateStatus>({ kind: "idle" });
let pending: Update | null = null;

export function useUpdateStatus() {
  return useStore(store);
}

/** Asks GitHub Releases for a newer version. Returns it, or null. */
export async function checkForUpdates(): Promise<string | null> {
  store.set({ kind: "checking" });
  try {
    pending = await check();
    store.set(pending ? { kind: "available", version: pending.version } : { kind: "upToDate" });
    return pending?.version ?? null;
  } catch (e) {
    store.set({ kind: "error", message: String(e) });
    return null;
  }
}

/** Downloads the pending update, installs it and restarts Duri. */
export async function installUpdate() {
  const update = pending;
  if (!update) return;
  let total = 0;
  let done = 0;
  store.set({ kind: "downloading", version: update.version, progress: null });
  try {
    await update.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? 0;
      if (e.event === "Progress") {
        done += e.data.chunkLength;
        store.set({ kind: "downloading", version: update.version, progress: total ? done / total : null });
      }
    });
    await relaunch();
  } catch (e) {
    store.set({ kind: "error", message: String(e) });
  }
}

const FIRST_CHECK_MS = 30_000;
const CHECK_EVERY_MS = 12 * 60 * 60 * 1000;
const NOTIFIED_KEY = "duri.update.notified";

async function notify(version: string) {
  try {
    if (localStorage.getItem(NOTIFIED_KEY) === version) return;
    localStorage.setItem(NOTIFIED_KEY, version);
  } catch {
    // storage unavailable: notify anyway
  }
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (!granted) return;
    sendNotification({ title: i18n.t("updates.notifyTitle", { version }), body: i18n.t("updates.notifyBody") });
  } catch {
    // best effort
  }
}

let started = false;

/**
 * Checks shortly after launch and twice a day (Settings › Check for Updates
 * Automatically), and tells the person once per new version. Installing is
 * always their choice, from Settings › About.
 */
export function startUpdateChecks() {
  if (started || import.meta.env.DEV) return;
  started = true;
  const run = async () => {
    if (!settingsStore.get().autoUpdateCheck) return;
    const version = await checkForUpdates();
    if (version) notify(version);
  };
  setTimeout(run, FIRST_CHECK_MS);
  setInterval(run, CHECK_EVERY_MS);
}
