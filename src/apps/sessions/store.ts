import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

export type Agent = "claude" | "codex";
export type Status = "idle" | "working" | "waiting" | "done";

/** Mirrors `Session` in src-tauri/src/apps/sessions.rs. */
export interface Session {
  id: string;
  agent: Agent;
  project: string;
  cwd: string;
  status: Status;
  detail: string | null;
  prompt: string | null;
  startedAt: number;
  updatedAt: number;
  statusSince: number;
}

interface SessionsStore {
  sessions: Session[];
  /** Alerts the person has already seen (`id:statusSince`). */
  acknowledged: string[];
  /** Session picked in the expanded view; null follows the most urgent one. */
  selectedId: string | null;
}

export interface HookStatus {
  installed: boolean;
  serverRunning: boolean;
}

const store = createStore<SessionsStore>({ sessions: [], acknowledged: [], selectedId: null });
const hooks = createStore<HookStatus | null>(null);

export const ackKey = (s: Session) => `${s.id}:${s.statusSince}`;

/** How long a finished session keeps the pill after it is done. */
export const DONE_VISIBLE_MS = 60_000;

let started = false;

export function startSessions() {
  if (started) return;
  started = true;
  invoke<Session[]>("sessions_get").then((sessions) => store.set((v) => ({ ...v, sessions })));
  listen<Session[]>("sessions://changed", (e) => store.set((v) => ({ ...v, sessions: e.payload })));
  refreshHooks();
}

export function useSessions() {
  return useStore(store);
}

export function useHookStatus() {
  return useStore(hooks);
}

export function refreshHooks() {
  return invoke<HookStatus>("claude_hooks_status")
    .then((s) => hooks.set(s))
    .catch(() => hooks.set({ installed: false, serverRunning: false }));
}

export function needsYou(s: Session, acknowledged: string[]) {
  return s.status === "waiting" && !acknowledged.includes(ackKey(s));
}

let lastFocus: string | null = null;

/**
 * The session the island talks about. A session that needs you always wins;
 * otherwise the one already shown stays, so busy sessions that keep flipping
 * between working and done don't make the island swap text back and forth.
 */
export function focusSession(sessions: Session[], acknowledged: string[]): Session | undefined {
  const pick =
    sessions.find((s) => needsYou(s, acknowledged)) ??
    sessions.find((s) => s.id === lastFocus && s.status !== "idle") ??
    sessions[0];
  lastFocus = pick?.id ?? null;
  return pick;
}

export const sessionActions = {
  select(id: string | null) {
    store.set((v) => ({ ...v, selectedId: id }));
  },
  acknowledge(s: Session) {
    store.set((v) => ({ ...v, acknowledged: [...v.acknowledged, ackKey(s)].slice(-100) }));
  },
  dismiss: (id: string) => invoke("session_dismiss", { id }),
  async installHooks() {
    await invoke("claude_hooks_install");
    await refreshHooks();
  },
  async uninstallHooks() {
    await invoke("claude_hooks_uninstall");
    await refreshHooks();
  },
};
