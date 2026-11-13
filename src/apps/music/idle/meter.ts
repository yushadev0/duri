import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

/** Last output peak from `audio://level` (src-tauri/src/apps/meter.rs), 0 – 1. */
let latest = 0;

export const getLevel = () => latest;

/** Keeps the backend's output meter running while `on`. */
export function useAudioMeter(on: boolean) {
  useEffect(() => {
    if (!on) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    listen<number>("audio://level", (e) => {
      latest = e.payload;
    }).then((f) => (alive ? (unlisten = f) : f()));
    invoke("audio_meter", { on: true }).catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
      latest = 0;
      invoke("audio_meter", { on: false }).catch(() => {});
    };
  }, [on]);
}

export interface Envelope {
  /** Recent loudest peak, decaying slowly: quiet songs still fill the circle. */
  peak: number;
  /** Smoothed, normalised level, 0 – 1. */
  value: number;
}

const PEAK_HALF_LIFE_MS = 4000;
const ATTACK_MS = 40;
const RELEASE_MS = 240;
/** Below this the music is treated as silence, not boosted. */
const FLOOR = 0.04;

/**
 * Follows the meter like a VU needle: rises fast, falls slowly, and scales
 * to the song's own loudness. Slow on purpose, so the circle breathes with
 * the music instead of flickering.
 */
export function follow(env: Envelope, raw: number, dtMs: number): Envelope {
  const peak = Math.max(raw, env.peak * Math.pow(0.5, dtMs / PEAK_HALF_LIFE_MS), FLOOR);
  const target = Math.min(1, Math.max(0, raw / peak));
  const k = 1 - Math.pow(0.5, dtMs / (target > env.value ? ATTACK_MS : RELEASE_MS));
  return { peak, value: env.value + (target - env.value) * k };
}
