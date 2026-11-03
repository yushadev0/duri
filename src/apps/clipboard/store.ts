import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

export type ClipKind = "text" | "image" | "files";
export type Filter = "all" | ClipKind;

/** Mirrors `Clip` in src-tauri/src/apps/clipboard.rs. */
export interface Clip {
  id: string;
  kind: ClipKind;
  text: string | null;
  files: string[];
  thumb: string | null;
  width: number | null;
  height: number | null;
  pinned: boolean;
  createdAt: number;
  usedAt: number;
}

interface ClipboardStore {
  items: Clip[];
  query: string;
  filter: Filter;
}

const store = createStore<ClipboardStore>({ items: [], query: "", filter: "all" });
let seq = 0;

async function refresh() {
  const { query, filter } = store.get();
  const mine = ++seq;
  const items = await invoke<Clip[]>("clips_list", {
    query: query.trim() || null,
    kind: filter === "all" ? null : filter,
  }).catch(() => null);
  // Ignore answers to queries that were superseded while typing.
  if (items && mine === seq) store.set((v) => ({ ...v, items }));
}

/** Set by the clipboard shortcut: the next expanded view focuses search. */
let focusSearchNext = false;
export function requestSearchFocus() {
  focusSearchNext = true;
}
export function takeSearchFocus() {
  const want = focusSearchNext;
  focusSearchNext = false;
  return want;
}

let started = false;

export function startClipboard() {
  if (started) return;
  started = true;
  refresh();
  listen("clipboard://changed", () => refresh());
}

export function useClipboard() {
  return useStore(store);
}

export const clipboardActions = {
  setQuery(query: string) {
    store.set((v) => ({ ...v, query }));
    refresh();
  },
  setFilter(filter: Filter) {
    store.set((v) => ({ ...v, filter }));
    refresh();
  },
  paste: (id: string) => invoke("clip_paste", { id }),
  copy: (id: string) => invoke("clip_copy", { id }),
  setPinned: (id: string, pinned: boolean) => invoke("clip_set_pinned", { id, pinned }),
  remove: (id: string) => invoke("clip_delete", { id }),
  clear: () => invoke("clips_clear"),
};

export function fileName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/** One-line preview with whitespace collapsed. */
export function preview(text: string, max = 240) {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}
