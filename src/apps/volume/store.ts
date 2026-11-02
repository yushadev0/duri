import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `VolumeState` in src-tauri/src/apps/volume.rs. */
export interface VolumeState {
  available: boolean;
  /** 0 – 1 */
  level: number;
  muted: boolean;
}

interface VolumeStore {
  state: VolumeState;
  /** True for a moment after the volume changes: the pill shows the level. */
  hud: boolean;
}

/** How long the volume indicator stays in the pill after a change. */
export const HUD_MS = 1600;

const store = createStore<VolumeStore>({ state: { available: false, level: 0, muted: false }, hud: false });
let hudTimer: ReturnType<typeof setTimeout> | undefined;
let started = false;

export function startVolume() {
  if (started) return;
  started = true;
  invoke<VolumeState>("volume_get").then((state) => store.set((v) => ({ ...v, state })));
  listen<VolumeState>("volume://changed", (e) => {
    const first = !store.get().state.available;
    store.set((v) => ({ ...v, state: e.payload, hud: !first && e.payload.available }));
    clearTimeout(hudTimer);
    hudTimer = setTimeout(() => store.set((v) => ({ ...v, hud: false })), HUD_MS);
  });
}

export function useVolume() {
  return useStore(store);
}

export const volumeActions = {
  set(level: number) {
    store.set((v) => ({ ...v, state: { ...v.state, level, muted: v.state.muted && level === 0 } }));
    return invoke("volume_set", { level });
  },
  toggleMute: () => invoke("volume_toggle_mute"),
};
