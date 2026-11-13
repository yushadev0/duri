import { Music2 } from "lucide-react";
import { useLayoutEffect, useRef, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { formatTrackTime } from "../../../lib/format";
import { useAlignedNow } from "../../../lib/ticks";
import { useLineIndex, type Timeline } from "../lyrics";
import { currentPosition, musicActions, useMusic } from "../store";
import styles from "./idle.module.css";

export function Artwork({ size }: { size: string }) {
  const { artwork } = useMusic();
  const style = { "--size": size } as CSSProperties;
  return artwork ? (
    <img className={styles.artwork} src={artwork} alt="" style={style} draggable={false} />
  ) : (
    <span className={`${styles.artwork} ${styles.artworkEmpty}`} style={style} aria-hidden>
      <Music2 size="40%" />
    </span>
  );
}

function Progress() {
  const { state, accent } = useMusic();
  const now = useAlignedNow(state.playing, 1000);
  if (state.durationMs <= 0) return null;
  const pos = currentPosition(state, now);
  return (
    <div className={styles.progress}>
      <div className={styles.progressRail}>
        <div
          className={styles.progressFill}
          data-playing={state.playing}
          style={{ width: `${(pos / state.durationMs) * 100}%`, background: accent }}
        />
      </div>
      <div className={`${styles.progressTimes} tabular`}>
        <span>{formatTrackTime(pos)}</span>
        <span>−{formatTrackTime(state.durationMs - pos)}</span>
      </div>
    </div>
  );
}

/** Large cover with the song's name: the quiet variant, and the fallback without lyrics. */
export function CoverView({ note }: { note?: string }) {
  const { t } = useTranslation();
  const { state } = useMusic();
  return (
    <div className={styles.cover}>
      <Artwork size="min(46vh, 480px)" />
      <div className={styles.coverMeta}>
        <h1 className={styles.coverTitle}>{state.title || t("music.unknownTitle")}</h1>
        <p className={styles.coverArtist}>{state.artist}</p>
      </div>
      <Progress />
      {note && <p className={styles.note}>{note}</p>}
    </div>
  );
}

/** Small song info in a corner, for the variants where lyrics take the stage. */
export function NowPlaying() {
  const { t } = useTranslation();
  const { state } = useMusic();
  return (
    <div className={styles.nowPlaying}>
      <Artwork size="52px" />
      <div className={styles.nowMeta}>
        <span className={styles.nowTitle}>{state.title || t("music.unknownTitle")}</span>
        <span className={styles.nowArtist}>{state.artist}</span>
      </div>
    </div>
  );
}

/**
 * Lyrics, the calm variant: the current line is bright and centered and the
 * list glides to it. Clicking a line jumps there. Plain (untimed) lyrics are
 * shown as a page to scroll.
 */
export function LyricsView({ timeline, reduceMotion }: { timeline: Timeline; reduceMotion: boolean }) {
  const { t } = useTranslation();
  const { state } = useMusic();
  const idx = useLineIndex(timeline.lines, state, timeline.synced);
  const viewport = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = list.current;
    const vp = viewport.current;
    if (!el || !vp || !timeline.synced) return;
    const line = el.children[Math.max(0, idx)] as HTMLElement | undefined;
    if (!line) return;
    const y = vp.clientHeight * 0.42 - (line.offsetTop + line.offsetHeight / 2);
    el.style.transform = `translateY(${Math.round(y)}px)`;
  }, [idx, timeline]);

  return (
    <div className={styles.lyricsLayout}>
      <aside className={styles.lyricsSide}>
        <Artwork size="min(34vh, 340px)" />
        <h1 className={styles.sideTitle}>{state.title || t("music.unknownTitle")}</h1>
        <p className={styles.sideArtist}>{state.artist}</p>
        {!timeline.synced && <p className={styles.note}>{t("music.idle.unsynced")}</p>}
      </aside>
      <div ref={viewport} className={styles.lyricsViewport} data-synced={timeline.synced}>
        <div ref={list} className={styles.lyricsList} data-reduce={reduceMotion}>
          {timeline.lines.map((line, i) => {
            const when = !timeline.synced ? "plain" : i === idx ? "now" : i < idx ? "past" : "next";
            const seekable = timeline.synced && state.canSeek;
            return (
              <p
                key={i}
                className={styles.line}
                data-state={when}
                data-gap={!line.text}
                data-seekable={seekable}
                onClick={seekable ? () => musicActions.seek(line.t) : undefined}
              >
                {line.text || "♪"}
              </p>
            );
          })}
        </div>
      </div>
    </div>
  );
}
