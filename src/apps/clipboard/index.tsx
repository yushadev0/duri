import { ClipboardList, Copy, File, Image, LayoutList, Pin, PinOff, Search, Trash2, Type, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconButton } from "../../components/Button";
import { useIsland } from "../../island/context";
import { useAgo } from "../../lib/useAgo";
import type { DuriApp } from "../types";
import styles from "./clipboard.module.css";
import { clipboardActions, fileName, preview, startClipboard, useClipboard, type Clip, type Filter, takeSearchFocus } from "./store";

const ACCENT = "var(--accent-clipboard)";
const FILTERS: { id: Filter; Icon: typeof Type }[] = [
  { id: "all", Icon: LayoutList },
  { id: "text", Icon: Type },
  { id: "image", Icon: Image },
  { id: "files", Icon: File },
];

function Leading({ clip }: { clip: Clip }) {
  if (clip.kind === "image" && clip.thumb) return <img className={styles.thumb} src={clip.thumb} alt="" draggable={false} />;
  return (
    <span className={styles.kindIcon} aria-hidden>
      {clip.kind === "files" ? <File size={16} /> : <Type size={16} />}
    </span>
  );
}

function Summary({ clip }: { clip: Clip }) {
  const { t } = useTranslation();
  if (clip.kind === "text") return <span className={styles.text}>{preview(clip.text ?? "")}</span>;
  if (clip.kind === "image")
    return (
      <span className={styles.text}>
        {t("clipboard.image")}
        {clip.width && clip.height ? ` · ${clip.width}×${clip.height}` : ""}
      </span>
    );
  const [first, ...rest] = clip.files;
  return (
    <span className={styles.text}>
      {fileName(first ?? "")}
      {rest.length > 0 && <span className={styles.more}> +{rest.length}</span>}
    </span>
  );
}

function Row({ clip, onPaste }: { clip: Clip; onPaste: (c: Clip) => void }) {
  const { t } = useTranslation();
  const ago = useAgo();
  return (
    <li className={styles.row}>
      <button
        type="button"
        className={styles.rowMain}
        onClick={(e) => {
          e.stopPropagation();
          onPaste(clip);
        }}
      >
        <Leading clip={clip} />
        <Summary clip={clip} />
        <span className={styles.ago}>{clip.pinned ? <Pin size={12} aria-label={t("clipboard.pin")} /> : ago(clip.usedAt)}</span>
      </button>
      <span className={styles.actions}>
        <IconButton
          label={clip.pinned ? t("clipboard.unpin") : t("clipboard.pin")}
          size={28}
          pressed={clip.pinned}
          onClick={() => clipboardActions.setPinned(clip.id, !clip.pinned)}
        >
          {clip.pinned ? <PinOff size={14} /> : <Pin size={14} />}
        </IconButton>
        <IconButton label={t("clipboard.copy")} size={28} onClick={() => clipboardActions.copy(clip.id)}>
          <Copy size={14} />
        </IconButton>
        <IconButton label={t("clipboard.delete")} size={28} onClick={() => clipboardActions.remove(clip.id)}>
          <X size={14} />
        </IconButton>
      </span>
    </li>
  );
}

/** Two-step clear: the first click asks, the second clears (unpinned only). */
function ClearButton() {
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
        clipboardActions.clear();
        setConfirming(false);
      }}
    >
      {t("clipboard.confirmClear")}
    </button>
  ) : (
    <IconButton label={t("clipboard.clear")} size={30} onClick={() => setConfirming(true)}>
      <Trash2 size={15} />
    </IconButton>
  );
}

export function ClipboardExpanded() {
  const { t } = useTranslation();
  const island = useIsland();
  const { items, query, filter } = useClipboard();
  const search = useRef<HTMLInputElement>(null);

  // Opened with the clipboard shortcut: type to search right away.
  useEffect(() => {
    if (takeSearchFocus()) search.current?.focus();
  }, []);

  const paste = (clip: Clip) => {
    island.collapse();
    clipboardActions.paste(clip.id);
  };

  return (
    <div className={styles.expanded}>
      <div className={styles.toolbar}>
        <label className={styles.search}>
          <Search size={14} aria-hidden />
          <input
            ref={search}
            type="search"
            value={query}
            placeholder={t("clipboard.search")}
            aria-label={t("clipboard.search")}
            spellCheck={false}
            onChange={(e) => clipboardActions.setQuery(e.target.value)}
            onFocus={() => island.setTyping(true)}
            onBlur={() => island.setTyping(false)}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Enter" && items[0]) paste(items[0]);
            }}
          />
        </label>
        <div className={styles.filters} role="group" aria-label={t("clipboard.filter.all")}>
          {FILTERS.map(({ id, Icon }) => (
            <IconButton
              key={id}
              label={t(`clipboard.filter.${id}`)}
              size={30}
              variant={filter === id ? "tinted" : "plain"}
              tone={ACCENT}
              pressed={filter === id}
              onClick={() => clipboardActions.setFilter(id)}
            >
              <Icon size={15} />
            </IconButton>
          ))}
        </div>
        <ClearButton />
      </div>
      {items.length ? (
        <ul className={styles.list}>
          {items.map((clip) => (
            <Row key={clip.id} clip={clip} onPaste={paste} />
          ))}
        </ul>
      ) : (
        <div className={styles.empty}>
          <ClipboardList size={26} color="var(--island-fg-3)" />
          <p className={styles.emptyTitle}>{query || filter !== "all" ? t("clipboard.noResults") : t("clipboard.empty")}</p>
          {!query && filter === "all" && <p className={styles.emptyHint}>{t("clipboard.emptyHint")}</p>}
        </div>
      )}
    </div>
  );
}

export function ClipboardCompact() {
  const { t } = useTranslation();
  const { items } = useClipboard();
  const latest = items[0];
  return (
    <div className={styles.compact}>
      <span className={styles.tile}>
        <ClipboardList size={22} color={ACCENT} />
      </span>
      <div className={styles.meta}>
        {latest ? <Summary clip={latest} /> : <span className={styles.text}>{t("clipboard.empty")}</span>}
        <span className={styles.sub}>{t("clipboard.items", { count: items.length })}</span>
      </div>
    </div>
  );
}

const Glyph = () => <ClipboardList size={18} color={ACCENT} />;

export const clipboardApp: DuriApp = {
  id: "clipboard",
  nameKey: "apps.clipboard.name",
  descriptionKey: "apps.clipboard.description",
  Icon: ClipboardList,
  accent: ACCENT,
  start: startClipboard,
  // History isn't a live activity: it never takes over the pill.
  useActivity: () => null,
  Pill: Glyph,
  pillWidth: 140,
  Minimal: Glyph,
  Compact: ClipboardCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: ClipboardExpanded,
};
