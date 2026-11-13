import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { MINIMAL_GAP, MINIMAL_SIZE, PILL_SCALE, PILL_SIDE, WINDOW_WIDTH } from "../../island/geometry";
import { formatClock, useHour12 } from "../../lib/clock";
import { useSettings } from "../../lib/settings";
import { useAlignedNow } from "../../lib/ticks";
import { useLineIndex, useLyrics, type LyricsStatus, type Timeline } from "./lyrics";
import styles from "./music.module.css";
import { useMusic } from "./store";

const NO_LINES: Timeline["lines"] = [];

/** Room for the line in the pill: about 7–8 words at 12 px. */
const LINE_WIDTH = 400;
const CLOCK_WIDTH = 52;

/** Lyrics are on for the pill and this song has timed ones. */
export function usePillLyricsActive(): boolean {
  const settings = useSettings();
  const { state } = useMusic();
  const status = useLyrics(state, settings.musicPillLyrics);
  return settings.musicPillLyrics && status.kind === "ready" && status.timeline.synced;
}

/**
 * The music pill's width while it shows lyrics: wide enough for one line,
 * but never so wide that the bubble beside it (another app's activity)
 * would leave the island window. Steady for the whole song.
 */
export function useMusicPillWidth(): number | null {
  const settings = useSettings();
  const active = usePillLyricsActive();
  if (!active) return null;
  const zoom = PILL_SCALE[settings.pillSize] ?? 1;
  const wanted = LINE_WIDTH + (settings.musicPillClock ? CLOCK_WIDTH : 0) + 2 * PILL_SIDE;
  const room = Math.floor((WINDOW_WIDTH - 2 * (MINIMAL_GAP + MINIMAL_SIZE * zoom) - 8) / zoom);
  return Math.min(wanted, room);
}

/** The line being sung, or -1; re-renders only when it changes. */
function useCurrentLine(status: LyricsStatus) {
  const { state } = useMusic();
  const timeline = status.kind === "ready" ? status.timeline : null;
  const idx = useLineIndex(timeline?.lines ?? NO_LINES, state, !!timeline?.synced);
  return { timeline, idx };
}

function PillClock() {
  const { i18n } = useTranslation();
  const hour12 = useHour12();
  const now = new Date(useAlignedNow(true, 60_000));
  return <span className={`${styles.pillClock} tabular`}>{formatClock(now, i18n.language, hour12)}</span>;
}

/**
 * Middle of the music pill (Settings › Apps › Music): the line being sung,
 * on one line, with the time beside it if asked. The pill widens for it
 * (`useMusicPillWidth`); a line longer than that is cut with an ellipsis.
 * Between lines the usual idle info stays.
 */
export function MusicPillCenter({ fallback, side }: { fallback: ReactNode; side: number }) {
  const settings = useSettings();
  const { state } = useMusic();
  const status = useLyrics(state, settings.musicPillLyrics);
  const { timeline, idx } = useCurrentLine(status);
  const text = timeline?.synced && idx >= 0 ? timeline.lines[idx].text : "";
  if (!settings.musicPillLyrics || !text) return <>{fallback}</>;
  return (
    <div className={styles.pillLyrics} style={{ left: side, right: side }}>
      <span key={idx} className={styles.pillLine}>
        {text}
      </span>
      {settings.musicPillClock && <PillClock />}
    </div>
  );
}

function statusText(status: LyricsStatus, t: (k: string) => string): string {
  switch (status.kind) {
    case "loading":
      return t("music.idle.searching");
    case "instrumental":
      return t("music.idle.instrumental");
    case "error":
      return t("music.idle.error");
    case "ready":
      return t("music.idle.unsynced");
    default:
      return t("music.idle.noLyrics");
  }
}

/**
 * Three lines of lyrics in the expanded view: the one before, the one being
 * sung (bright) and the next. A status line stands in for songs without
 * timed lyrics, so the controls below never move.
 */
export function ExpandedLyrics() {
  const { t } = useTranslation();
  const { state } = useMusic();
  const status = useLyrics(state, true);
  const { timeline, idx } = useCurrentLine(status);

  if (!timeline?.synced) {
    return (
      <div className={styles.lyrics} aria-live="off">
        <span className={styles.lyricsStatus}>{statusText(status, t)}</span>
      </div>
    );
  }
  const line = (i: number) => (i >= 0 && i < timeline.lines.length ? timeline.lines[i].text || "♪" : "");
  return (
    <div className={styles.lyrics} aria-live="off">
      <div key={idx} className={styles.lyricsRows}>
        <span className={styles.lyricsPrev}>{line(idx - 1)}</span>
        <span className={styles.lyricsNow}>{idx >= 0 ? line(idx) : "♪"}</span>
        <span className={styles.lyricsNext}>{line(idx + 1)}</span>
      </div>
    </div>
  );
}
