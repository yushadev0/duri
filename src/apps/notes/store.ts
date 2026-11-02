import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

/** Mirrors `Note` in src-tauri/src/apps/notes.rs. */
export interface Note {
  id: string;
  body: string;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
}

interface NotesStore {
  notes: Note[];
  query: string;
  /** The editor is open when this isn't null; `id` is null for an unsaved new note. */
  editing: { id: string | null; draft: string } | null;
}

const store = createStore<NotesStore>({ notes: [], query: "", editing: null });
let seq = 0;

async function refresh() {
  const mine = ++seq;
  const q = store.get().query.trim();
  const notes = await invoke<Note[]>("notes_list", { query: q || null }).catch(() => null);
  if (notes && mine === seq) store.set((v) => ({ ...v, notes }));
}

let started = false;

export function startNotes() {
  if (started) return;
  started = true;
  refresh();
  listen("notes://changed", () => refresh());
}

export function useNotes() {
  return useStore(store);
}

export function noteTitle(body: string) {
  return body.split("\n").find((l) => l.trim())?.trim() ?? "";
}

export function notePreview(body: string) {
  const lines = body.split("\n").filter((l) => l.trim());
  return lines.slice(1).join(" ").replace(/\s+/g, " ").trim();
}

// Saving: the editor writes into `draft`; `save()` persists it. Saves are
// chained so a create always finishes before the update that follows it.
let chain: Promise<unknown> = Promise.resolve();

function persist(): Promise<unknown> {
  chain = chain.then(async () => {
    const editing = store.get().editing;
    if (!editing) return;
    const { id, draft } = editing;
    if (id === null) {
      if (!draft.trim()) return;
      const note = await invoke<Note>("note_create", { body: draft });
      store.set((v) => (v.editing && v.editing.id === null ? { ...v, editing: { ...v.editing, id: note.id } } : v));
    } else {
      await invoke("note_update", { id, body: draft });
    }
  }).catch(() => {});
  return chain;
}

let timer: ReturnType<typeof setTimeout> | undefined;

export const notesActions = {
  setQuery(query: string) {
    store.set((v) => ({ ...v, query }));
    refresh();
  },
  open(note: Note) {
    store.set((v) => ({ ...v, editing: { id: note.id, draft: note.body } }));
  },
  create() {
    store.set((v) => ({ ...v, editing: { id: null, draft: "" } }));
  },
  /** Updates the draft and saves shortly after typing pauses. */
  edit(draft: string) {
    store.set((v) => (v.editing ? { ...v, editing: { ...v.editing, draft } } : v));
    clearTimeout(timer);
    timer = setTimeout(persist, 400);
  },
  /** Saves now (used when leaving the editor or collapsing the island). */
  flush() {
    clearTimeout(timer);
    return persist();
  },
  async close() {
    await notesActions.flush();
    store.set((v) => ({ ...v, editing: null }));
  },
  async remove(id: string | null) {
    clearTimeout(timer);
    store.set((v) => ({ ...v, editing: null }));
    if (id) await invoke("note_delete", { id });
  },
  setPinned: (id: string, pinned: boolean) => invoke("note_set_pinned", { id, pinned }),
};
