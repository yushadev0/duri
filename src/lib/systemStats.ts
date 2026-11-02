import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "./store";

/** Mirrors `Stats` in src-tauri/src/system.rs (percentages). */
export interface SystemStats {
  cpu: number;
  memory: number;
}

const store = createStore<SystemStats>({ cpu: 0, memory: 0 });
let started = false;

export function startSystemStats() {
  if (started) return;
  started = true;
  invoke<SystemStats>("system_stats").then((s) => store.set(s));
  listen<SystemStats>("system://stats", (e) => store.set(e.payload));
}

export function useSystemStats() {
  return useStore(store);
}
