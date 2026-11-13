import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
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
/** The version whose download or install failed, for the island's prompt. */
const failedStore = createStore<string | null>(null);

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
  failedStore.set(null);
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
    failedStore.set(update.version);
    store.set({ kind: "error", message: String(e) });
  }
}

/** Versions the person said "Not Now" to; each is asked about once. */
const DISMISSED_KEY = "duri.update.dismissed";

function readDismissed(): string | null {
  try {
    return localStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

const dismissedStore = createStore<string | null>(readDismissed());

/**
 * The version the island should ask about (src/apps/update): found by a
 * check and not turned down yet. Stays while it downloads, so the progress
 * shows in the same place.
 */
export type UpdatePrompt =
  | { kind: "available"; version: string }
  | { kind: "downloading"; version: string; progress: number | null }
  | { kind: "failed"; version: string };

export function useUpdatePrompt(): UpdatePrompt | null {
  const status = useStore(store);
  const dismissed = useStore(dismissedStore);
  const failed = useStore(failedStore);
  if (status.kind === "downloading") return status;
  if (failed && failed !== dismissed) return { kind: "failed", version: failed };
  if (status.kind === "available" && status.version !== dismissed) return status;
  return null;
}

/** "Not Now": don't ask about this version again (Settings › About still has it). */
export function dismissUpdate(version: string) {
  if (failedStore.get() === version) failedStore.set(null);
  try {
    localStorage.setItem(DISMISSED_KEY, version);
  } catch {
    // storage unavailable: dismissed until Duri restarts
  }
  dismissedStore.set(version);
}

/** In development, checks run only with VITE_DURI_UPDATE_CHECK=1 (to try the prompt). */
const DEV_CHECKS = import.meta.env.DEV && import.meta.env.VITE_DURI_UPDATE_CHECK === "1";
const FIRST_CHECK_MS = DEV_CHECKS ? 3_000 : 30_000;
const CHECK_EVERY_MS = 12 * 60 * 60 * 1000;

let started = false;

/**
 * Checks shortly after launch and twice a day (Settings › Check for Updates
 * Automatically). A new version opens the island and asks: Download or Not
 * Now (src/apps/update). Installing is always the person's choice.
 */
export function startUpdateChecks() {
  if (started || (import.meta.env.DEV && !DEV_CHECKS)) return;
  started = true;
  const run = async () => {
    if (!settingsStore.get().autoUpdateCheck) return;
    // Don't interrupt a download in progress.
    if (store.get().kind === "downloading") return;
    await checkForUpdates();
  };
  setTimeout(run, FIRST_CHECK_MS);
  setInterval(run, CHECK_EVERY_MS);
}
