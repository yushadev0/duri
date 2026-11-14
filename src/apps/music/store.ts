import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";
import { extractAccent, NEUTRAL_ACCENT } from "./accent";

/** Mirrors `MusicState` in src-tauri/src/apps/music.rs. */
export interface MusicState {
  available: boolean;
  sourceAppId: string;
  title: string;
  artist: string;
  album: string;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  positionUpdatedAt: number;
  canToggle: boolean;
  canNext: boolean;
  canPrev: boolean;
  canSeek: boolean;
  sources: string[];
  trackKey: string;
}

interface Artwork {
  trackKey: string;
  dataUrl: string | null;
}

interface MusicStore {
  state: MusicState;
  artwork: string | null;
  accent: string;
}

const EMPTY: MusicState = {
  available: false,
  sourceAppId: "",
  title: "",
  artist: "",
  album: "",
  playing: false,
  positionMs: 0,
  durationMs: 0,
  positionUpdatedAt: 0,
  canToggle: false,
  canNext: false,
  canPrev: false,
  canSeek: false,
  sources: [],
  trackKey: "",
};

const store = createStore<MusicStore>({ state: EMPTY, artwork: null, accent: NEUTRAL_ACCENT });
let artworkKey = "";

async function applyArtwork(a: Artwork) {
  artworkKey = a.trackKey;
  store.set((v) => ({ ...v, artwork: a.dataUrl }));
  let accent = NEUTRAL_ACCENT;
  if (a.dataUrl) {
    try {
      accent = await extractAccent(a.dataUrl);
    } catch {
      // keep neutral
    }
  }
  if (artworkKey === a.trackKey) store.set((v) => ({ ...v, accent }));
}

let started = false;

export function startMusic() {
  if (started) return;
  started = true;
  invoke<{ state: MusicState; artwork: Artwork }>("music_get").then((s) => {
    store.set((v) => ({ ...v, state: s.state }));
    applyArtwork(s.artwork);
  });
  listen<MusicState>("music://state", (e) => store.set((v) => ({ ...v, state: e.payload })));
  listen<Artwork>("music://artwork", (e) => applyArtwork(e.payload));
}

/** Current value outside React (render loops that read it every frame). */
export function getMusic(): MusicStore {
  return store.get();
}

/** Calls `listener` whenever the music state changes. */
export const subscribeMusic = (listener: () => void) => store.subscribe(listener);

export function useMusic() {
  return useStore(store);
}

/** Position extrapolated from the last report, clamped to the track length. */
export function currentPosition(s: MusicState, now: number): number {
  const elapsed = s.playing ? Math.max(0, now - s.positionUpdatedAt) : 0;
  return Math.min(s.durationMs, s.positionMs + elapsed);
}

const patchState = (patch: Partial<MusicState>) =>
  store.set((v) => ({ ...v, state: { ...v.state, ...patch } }));

export const musicActions = {
  toggle() {
    const s = store.get().state;
    // Optimistic: freeze/resume the position locally until the player reports back.
    const now = Date.now();
    patchState({ playing: !s.playing, positionMs: currentPosition(s, now), positionUpdatedAt: now });
    return invoke("music_control", { command: "toggle" });
  },
  next: () => invoke("music_control", { command: "next" }),
  prev: () => invoke("music_control", { command: "prev" }),
  seek(positionMs: number) {
    patchState({ positionMs, positionUpdatedAt: Date.now() });
    return invoke("music_seek", { positionMs: Math.round(positionMs) });
  },
  selectSource: (sourceAppId: string | null) => invoke("music_select_source", { sourceAppId }),
  /** Brings the player app (Spotify, the browser…) to the front. */
  openPlayer: () => invoke("music_open_player"),
};
