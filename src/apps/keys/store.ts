import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `LockKey` in src-tauri/src/apps/keys.rs. */
export type LockKey = "caps" | "insert" | "scroll";

export const LOCK_KEYS: LockKey[] = ["caps", "insert", "scroll"];

/** Mirrors `LockEvent` in src-tauri/src/apps/keys.rs. */
export interface LockEvent {
  key: LockKey;
  on: boolean;
}

interface KeysStore {
  states: Record<LockKey, boolean>;
  /** The key just pressed, shown in the pill for a moment. */
  hud: (LockEvent & { seq: number }) | null;
}

/** Same as the volume indicator. */
export const HUD_MS = 1600;

const store = createStore<KeysStore>({ states: { caps: false, insert: false, scroll: false }, hud: null });
let hudTimer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;
let started = false;

export function startKeys() {
  if (started) return;
  started = true;
  invoke<Record<LockKey, boolean>>("keys_get").then((states) => store.set((v) => ({ ...v, states })));
  listen<LockEvent>("keys://lock", (e) => {
    const { key, on } = e.payload;
    store.set((v) => ({ states: { ...v.states, [key]: on }, hud: { key, on, seq: ++seq } }));
    clearTimeout(hudTimer);
    hudTimer = setTimeout(() => store.set((v) => ({ ...v, hud: null })), HUD_MS);
  });
}

export function useKeys() {
  return useStore(store);
}
