import { ChevronLeft, NotebookPen, Pin, PinOff, Plus, Search, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconButton } from "../../components/Button";
import { useIsland } from "../../island/context";
import { useAgo } from "../../lib/useAgo";
import type { DuriApp } from "../types";
import styles from "./notes.module.css";
import { notePreview, notesActions, noteTitle, startNotes, useNotes, type Note } from "./store";

const ACCENT = "var(--accent-notes)";

function DeleteButton({ onDelete }: { onDelete: () => void }) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    if (!confirming) return;
    const id = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(id);
  }, [confirming]);
  return confirming ? (
    <button
      type="button"
      className={styles.confirm}
      onClick={(e) => {
        e.stopPropagation();
        onDelete();
      }}
    >
      {t("notes.confirmDelete")}
    </button>
  ) : (
    <IconButton label={t("notes.delete")} size={30} onClick={() => setConfirming(true)}>
      <Trash2 size={15} />
    </IconButton>
  );
}

function Editor() {
  const { t } = useTranslation();
  const island = useIsland();
  const { editing, notes } = useNotes();
  const area = useRef<HTMLTextAreaElement>(null);
  const saved = editing?.id ? notes.find((n) => n.id === editing.id) : undefined;

  // Focus the text right away so typing just works.
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  // Whatever happens (back, collapse, Esc), the draft is saved.
  useEffect(() => () => void notesActions.flush(), []);

  if (!editing) return null;
  return (
    <div className={styles.editor}>
      <div className={styles.toolbar}>
        <IconButton label={t("notes.back")} size={30} onClick={() => notesActions.close()}>
          <ChevronLeft size={18} />
        </IconButton>
        <span className={styles.editorTitle}>{noteTitle(editing.draft) || t("notes.untitled")}</span>
        {saved && (
          <IconButton
            label={saved.pinned ? t("notes.unpin") : t("notes.pin")}
            size={30}
            pressed={saved.pinned}
            onClick={() => notesActions.setPinned(saved.id, !saved.pinned)}
          >
            {saved.pinned ? <PinOff size={15} /> : <Pin size={15} />}
          </IconButton>
        )}
        <DeleteButton onDelete={() => notesActions.remove(editing.id)} />
      </div>
      <textarea
        ref={area}
        className={styles.area}
        value={editing.draft}
        placeholder={t("notes.placeholder")}
        aria-label={noteTitle(editing.draft) || t("notes.untitled")}
        spellCheck={false}
        onChange={(e) => notesActions.edit(e.target.value)}
        onFocus={() => island.setTyping(true)}
        onBlur={() => island.setTyping(false)}
        onClick={(e) => e.stopPropagation()}
      />
    </div>
  );
}

function Row({ note }: { note: Note }) {
  const { t } = useTranslation();
  const ago = useAgo();
  const preview = notePreview(note.body);
  return (
    <li>
      <button
        type="button"
        className={styles.row}
        onClick={(e) => {
          e.stopPropagation();
          notesActions.open(note);
        }}
      >
        <span className={styles.rowText}>
          <span className={styles.rowTitle}>{noteTitle(note.body) || t("notes.untitled")}</span>
          {preview && <span className={styles.rowPreview}>{preview}</span>}
        </span>
        <span className={styles.ago}>
          {note.pinned ? <Pin size={12} aria-label={t("notes.pin")} /> : ago(note.updatedAt)}
        </span>
      </button>
    </li>
  );
}

function List() {
  const { t } = useTranslation();
  const island = useIsland();
  const { notes, query } = useNotes();
  return (
    <div className={styles.listView}>
      <div className={styles.toolbar}>
        <label className={styles.search}>
          <Search size={14} aria-hidden />
          <input
            type="search"
            value={query}
            placeholder={t("notes.search")}
            aria-label={t("notes.search")}
            spellCheck={false}
            onChange={(e) => notesActions.setQuery(e.target.value)}
            onFocus={() => island.setTyping(true)}
            onBlur={() => island.setTyping(false)}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Enter" && notes[0]) notesActions.open(notes[0]);
            }}
          />
        </label>
        <IconButton label={t("notes.new")} size={30} variant="tinted" tone={ACCENT} onClick={notesActions.create}>
          <Plus size={17} />
        </IconButton>
      </div>
      {notes.length ? (
        <ul className={styles.list}>
          {notes.map((n) => (
            <Row key={n.id} note={n} />
          ))}
        </ul>
      ) : (
        <div className={styles.empty}>
          <NotebookPen size={26} color="var(--island-fg-3)" />
          <p className={styles.emptyTitle}>{query ? t("notes.noResults") : t("notes.empty")}</p>
          {!query && <p className={styles.emptyHint}>{t("notes.emptyHint")}</p>}
        </div>
      )}
    </div>
  );
}

export function NotesExpanded() {
  const { editing } = useNotes();
  return editing ? <Editor /> : <List />;
}

export function NotesCompact() {
  const { t } = useTranslation();
  const { notes } = useNotes();
  const latest = notes[0];
  return (
    <div className={styles.compact}>
      <span className={styles.tile}>
        <NotebookPen size={22} color={ACCENT} />
      </span>
      <div className={styles.meta}>
        <span className={styles.rowTitle}>{latest ? noteTitle(latest.body) : t("notes.empty")}</span>
        <span className={styles.rowPreview}>{t("notes.count", { count: notes.length })}</span>
      </div>
    </div>
  );
}

const Glyph = () => <NotebookPen size={18} color={ACCENT} />;

export const notesApp: DuriApp = {
  id: "notes",
  nameKey: "apps.notes.name",
  descriptionKey: "apps.notes.description",
  Icon: NotebookPen,
  accent: ACCENT,
  start: startNotes,
  useActivity: () => null,
  Pill: Glyph,
  pillWidth: 140,
  Minimal: Glyph,
  Compact: NotesCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: NotesExpanded,
  expandedSize: { w: 460, h: 300 },
};
