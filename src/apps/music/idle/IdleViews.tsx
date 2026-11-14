import { Music2 } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
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

/** Tonearm angles (deg, clockwise from straight down): off the record, outer groove, last groove. */
const ARM_REST = -8;
const ARM_START = 5;
const ARM_END = 28.5;
/** The arm waits for the flip to land before it swings in. */
const ARM_DELAY_MS = 650;

/**
 * The record's tonearm, in the cover's 0–100 box with its pivot off the top
 * right corner. It lowers onto the record while the song plays, creeps
 * toward the label as it goes on, and swings back off when it stops.
 */
function Tonearm({ on }: { on: boolean }) {
  const { state } = useMusic();
  const playing = on && state.playing;
  const [landed, setLanded] = useState(false);
  const now = useAlignedNow(playing, 1000);
  const down = playing && landed;

  useEffect(() => {
    if (!playing) return;
    const id = setTimeout(() => setLanded(true), ARM_DELAY_MS);
    return () => {
      clearTimeout(id);
      setLanded(false);
    };
  }, [playing]);

  const progress = state.durationMs > 0 ? Math.min(1, currentPosition(state, now) / state.durationMs) : 0;
  const angle = down ? ARM_START + (ARM_END - ARM_START) * progress : ARM_REST;
  return (
    <svg className={styles.tonearm} viewBox="0 0 100 100" data-down={down} aria-hidden>
      <g className={styles.arm} style={{ "--arm": `${angle}deg` } as CSSProperties}>
        <rect x="92.4" y="-8" width="5.2" height="7.5" rx="1.2" fill="#55555b" />
        <line x1="95" y1="5" x2="95" y2="66" stroke="#cfcfd4" strokeWidth="1.1" strokeLinecap="round" />
        <rect x="93" y="65" width="4.6" height="9.5" rx="0.9" fill="#dcdce0" transform="rotate(14 95 66)" />
        <circle cx="95" cy="5" r="5.2" fill="#232327" stroke="rgba(255,255,255,0.14)" strokeWidth="0.6" />
        <circle cx="95" cy="5" r="2.4" fill="#8e8e94" />
      </g>
    </svg>
  );
}

/**
 * The cover, and a little secret: tap it and it flips over into a vinyl
 * record (the cover as its label) that spins while the song plays. Tap again
 * to flip back.
 */
function FlipCover({ size }: { size: string }) {
  const { t } = useTranslation();
  const { state, artwork, accent } = useMusic();
  const [vinyl, setVinyl] = useState(false);
  return (
    <button
      type="button"
      className={styles.flip}
      style={{ "--size": size, "--accent": accent } as CSSProperties}
      data-vinyl={vinyl}
      aria-pressed={vinyl}
      aria-label={vinyl ? t("music.idle.showCover") : t("music.idle.showRecord")}
      onClick={() => setVinyl((v) => !v)}
    >
      <span className={styles.flipInner}>
        <span className={styles.flipFront}>
          <Artwork size={size} />
        </span>
        <span className={styles.flipBack} aria-hidden>
          <span className={styles.record} data-spinning={vinyl && state.playing}>
            <span className={styles.grooves} />
            {artwork ? (
              <img className={styles.label} src={artwork} alt="" draggable={false} />
            ) : (
              <span className={`${styles.label} ${styles.labelEmpty}`}>
                <Music2 size="45%" />
              </span>
            )}
            <span className={styles.spindle} />
          </span>
          <span className={styles.sheen} />
          <Tonearm on={vinyl} />
        </span>
      </span>
    </button>
  );
}

/** Arrow keys move this far on the progress bar. */
const SEEK_STEP_MS = 5000;

/** The song's progress; drag or click it to seek, or use the arrow keys. */
function Progress() {
  const { t } = useTranslation();
  const { state, accent } = useMusic();
  const now = useAlignedNow(state.playing, 1000);
  const [drag, setDrag] = useState<number | null>(null);
  const track = useRef<HTMLDivElement>(null);
  const duration = state.durationMs;
  if (duration <= 0) return null;
  const enabled = state.canSeek;
  const pos = drag ?? currentPosition(state, now);

  const toMs = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * duration;
  };
  const seekBy = (ms: number) => musicActions.seek(Math.min(duration, Math.max(0, currentPosition(state, Date.now()) + ms)));

  return (
    <div className={styles.progress}>
      <div
        ref={track}
        className={styles.progressTrack}
        data-enabled={enabled}
        data-dragging={drag !== null}
        role="slider"
        tabIndex={enabled ? 0 : -1}
        aria-label={t("music.seek")}
        aria-valuemin={0}
        aria-valuemax={Math.round(duration / 1000)}
        aria-valuenow={Math.round(pos / 1000)}
        aria-valuetext={formatTrackTime(pos)}
        aria-disabled={!enabled}
        onPointerDown={(e) => {
          if (!enabled || e.button !== 0) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          setDrag(toMs(e.clientX));
        }}
        onPointerMove={(e) => drag !== null && setDrag(toMs(e.clientX))}
        onPointerUp={(e) => {
          if (drag === null) return;
          musicActions.seek(toMs(e.clientX));
          setDrag(null);
        }}
        onPointerCancel={() => setDrag(null)}
        onKeyDown={(e) => {
          if (!enabled) return;
          const step = { ArrowLeft: -SEEK_STEP_MS, ArrowDown: -SEEK_STEP_MS, ArrowRight: SEEK_STEP_MS, ArrowUp: SEEK_STEP_MS }[
            e.key
          ];
          if (step) seekBy(step);
          else if (e.key === "Home") musicActions.seek(0);
          else if (e.key === "End") musicActions.seek(Math.max(0, duration - 1000));
          else return;
          e.preventDefault();
        }}
      >
        <div className={styles.progressRail}>
          <div
            className={styles.progressFill}
            data-playing={state.playing && drag === null}
            style={{ width: `${(pos / duration) * 100}%`, background: accent }}
          />
        </div>
      </div>
      <div className={`${styles.progressTimes} tabular`}>
        <span>{formatTrackTime(pos)}</span>
        <span>−{formatTrackTime(duration - pos)}</span>
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
      <FlipCover size="min(46vh, 480px)" />
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
