import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `Notice` in src-tauri/src/apps/notifications.rs. */
export interface Notice {
  id: number;
  app: string;
  appId: string;
  title: string;
  body: string;
  at: number;
  icon: string | null;
}

export type Access = "unknown" | "allowed" | "denied" | "unavailable";

/** Mirrors `Copied` in src-tauri/src/apps/clipboard.rs. */
export interface Copied {
  kind: "text" | "image" | "files";
  preview: string;
  count: number;
}

/** What the pill shows for a moment: a new notification, or "Copied". */
export type Flash = { kind: "notice"; notice: Notice; key: string } | { kind: "copied"; copied: Copied; key: string };

interface NotificationsStore {
  access: Access;
  /** Newest first, in memory only. */
  items: Notice[];
  flash: Flash | null;
}

/** How long a new notification, and the copy confirmation, hold the pill. */
export const NOTICE_MS = 5000;
export const COPIED_MS = 1600;

const store = createStore<NotificationsStore>({ access: "unknown", items: [], flash: null });
let flashTimer: ReturnType<typeof setTimeout> | undefined;
let seq = 0;

function flash(f: Flash, ms: number) {
  clearTimeout(flashTimer);
  store.set((v) => ({ ...v, flash: f }));
  flashTimer = setTimeout(() => store.set((v) => ({ ...v, flash: null })), ms);
}

let started = false;

export function startNotifications() {
  if (started) return;
  started = true;
  type Snapshot = { access: Access; items: Notice[] };
  invoke<Snapshot>("notifications_get").then((s) => store.set((v) => ({ ...v, ...s })));
  listen<Snapshot>("notifications://changed", (e) => store.set((v) => ({ ...v, ...e.payload })));
  listen<Notice>("notifications://new", (e) => flash({ kind: "notice", notice: e.payload, key: `n${e.payload.id}` }, NOTICE_MS));
  listen<Copied>("clipboard://copied", (e) => flash({ kind: "copied", copied: e.payload, key: `c${++seq}` }, COPIED_MS));
}

export function useNotifications() {
  return useStore(store);
}

export const notificationActions = {
  dismiss(id: number) {
    store.set((v) => ({ ...v, items: v.items.filter((n) => n.id !== id) }));
    return invoke("notification_dismiss", { id }).catch(() => {});
  },
  clear() {
    store.set((v) => ({ ...v, items: [] }));
    return invoke("notifications_clear").catch(() => {});
  },
};
