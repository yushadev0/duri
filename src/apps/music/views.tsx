import { invoke } from "@tauri-apps/api/core";
import { ChevronsUpDown, Maximize2, Music2, Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { IconButton } from "../../components/Button";
import { VolumeSlider } from "../../components/VolumeSlider";
import { formatTrackTime } from "../../lib/format";
import { useStep } from "../../lib/ticks";
import { useNow } from "../../lib/useNow";
import { ExpandedLyrics } from "./lyricsViews";
import styles from "./music.module.css";
import { sourceName } from "./sources";
import { currentPosition, musicActions, useMusic } from "./store";

const filled = { fill: "currentColor", strokeWidth: 1.5 } as const;

function Artwork({ size, radius }: { size: number; radius: number }) {
  const { artwork } = useMusic();
  const style: CSSProperties = { width: size, height: size, borderRadius: radius };
  return artwork ? (
    <img className={styles.artwork} src={artwork} alt="" style={style} draggable={false} />
  ) : (
    <span className={`${styles.artwork} ${styles.artworkEmpty}`} style={style} aria-hidden>
      <Music2 size={Math.round(size * 0.45)} />
    </span>
  );
}

/** Bar heights for the stepped equalizer, one row per step. */
const EQ_FRAMES = [
  [0.45, 0.9, 0.6, 0.35],
  [0.8, 0.5, 1, 0.6],
  [0.35, 0.75, 0.45, 0.95],
  [0.65, 1, 0.3, 0.7],
  [1, 0.4, 0.8, 0.45],
  [0.5, 0.65, 0.55, 1],
];
const EQ_STEP_MS = 250;

/**
 * Playing indicator. In the expanded view it's a smooth CSS animation; in
 * the always-visible pill it steps a few times a second instead, because a
 * running CSS animation repaints the whole transparent island window at the
 * display's refresh rate (most of Duri's idle CPU before 0.2).
 */
function Equalizer({
  playing,
  color,
  height = 14,
  smooth = false,
}: {
  playing: boolean;
  color: string;
  height?: number;
  smooth?: boolean;
}) {
  const reduceMotion = useReducedMotion();
  const step = useStep(playing && !smooth && !reduceMotion, EQ_STEP_MS);
  const frame = EQ_FRAMES[step % EQ_FRAMES.length];
  return (
    <span
      className={styles.eq}
      data-playing={playing}
      data-stepped={!smooth}
      style={{ height, color } as CSSProperties}
      aria-hidden
    >
      {frame.map((h, i) => (
        <i key={i} style={!smooth && playing && !reduceMotion ? { transform: `scaleY(${h})` } : undefined} />
      ))}
    </span>
  );
}

function PlayPause({ size, iconSize }: { size: number; iconSize: number }) {
  const { t } = useTranslation();
  const { state } = useMusic();
  return (
    <IconButton
      label={state.playing ? t("music.pause") : t("music.play")}
      size={size}
      disabled={!state.canToggle}
      onClick={musicActions.toggle}
    >
      {state.playing ? <Pause size={iconSize} {...filled} /> : <Play size={iconSize} {...filled} />}
    </IconButton>
  );
}

export function MusicPill() {
  const { state, accent } = useMusic();
  return (
    <div className={styles.pill}>
      <Artwork size={22} radius={7} />
      <Equalizer playing={state.playing} color={accent} />
    </div>
  );
}

export function MusicMinimal() {
  return <Artwork size={24} radius={12} />;
}

export function MusicCompact() {
  const { t } = useTranslation();
  const { state } = useMusic();
  if (!state.available) return <Empty compact />;
  return (
    <div className={styles.compact}>
      <Artwork size={48} radius={16} />
      <div className={styles.meta}>
        <span className={styles.title}>{state.title || t("music.unknownTitle")}</span>
        <span className={styles.artist}>{state.artist}</span>
      </div>
      <div className={styles.controls}>
        <PlayPause size={36} iconSize={18} />
        <IconButton label={t("music.next")} size={36} disabled={!state.canNext} onClick={musicActions.next}>
          <SkipForward size={18} {...filled} />
        </IconButton>
      </div>
    </div>
  );
}

function Empty({ compact }: { compact?: boolean }) {
  const { t } = useTranslation();
  return (
    <div className={compact ? styles.emptyCompact : styles.empty}>
      <span className={`${styles.artwork} ${styles.artworkEmpty}`} style={{ width: 44, height: 44, borderRadius: 14 }}>
        <Music2 size={20} />
      </span>
      <div className={styles.meta}>
        <span className={styles.title}>{t("music.nothingPlaying")}</span>
        <span className={styles.artist}>{t("music.nothingPlayingHint")}</span>
      </div>
    </div>
  );
}

function Scrubber() {
  const { t } = useTranslation();
  const { state, accent } = useMusic();
  const now = useNow(state.playing);
  const [drag, setDrag] = useState<number | null>(null);
  const track = useRef<HTMLDivElement>(null);
  const duration = state.durationMs;
  const position = drag ?? currentPosition(state, now);
  const enabled = state.canSeek && duration > 0;

  const toMs = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * duration;
  };

  if (duration <= 0) return <div className={styles.scrubberSpacer} />;

  return (
    <div className={styles.scrubber}>
      <div
        ref={track}
        className={styles.track}
        data-enabled={enabled}
        data-dragging={drag !== null}
        role="slider"
        aria-label={t("music.seek")}
        aria-valuemin={0}
        aria-valuemax={Math.round(duration / 1000)}
        aria-valuenow={Math.round(position / 1000)}
        aria-valuetext={formatTrackTime(position)}
        aria-disabled={!enabled}
        onClick={(e) => e.stopPropagation()}
        onPointerDown={(e) => {
          if (!enabled) return;
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
      >
        <div className={styles.rail}>
          <div className={styles.fill} style={{ width: `${(position / duration) * 100}%`, background: accent }} />
        </div>
      </div>
      <div className={`${styles.times} tabular`}>
        <span>{formatTrackTime(position)}</span>
        <span>−{formatTrackTime(duration - position)}</span>
      </div>
    </div>
  );
}

export function MusicExpanded() {
  const { t } = useTranslation();
  const { state, accent } = useMusic();
  if (!state.available) return <Empty />;

  const multipleSources = state.sources.length > 1;
  const nextSource = () => {
    const i = state.sources.indexOf(state.sourceAppId);
    musicActions.selectSource(state.sources[(i + 1) % state.sources.length]);
  };

  return (
    <div className={styles.expanded}>
      <div className={styles.head}>
        <button
          type="button"
          className={styles.coverButton}
          title={t("music.openPlayer", { app: sourceName(state.sourceAppId) })}
          aria-label={t("music.openPlayer", { app: sourceName(state.sourceAppId) })}
          onClick={(e) => {
            e.stopPropagation();
            musicActions.openPlayer().catch(() => {});
          }}
        >
          <Artwork size={64} radius={18} />
        </button>
        <div className={styles.meta}>
          <span className={styles.titleLarge}>{state.title || t("music.unknownTitle")}</span>
          <span className={styles.artist}>{state.artist}</span>
          {multipleSources ? (
            <button
              type="button"
              className={styles.source}
              title={t("music.switchSource")}
              aria-label={`${sourceName(state.sourceAppId)}. ${t("music.switchSource")}`}
              onClick={(e) => {
                e.stopPropagation();
                nextSource();
              }}
            >
              {sourceName(state.sourceAppId)}
              <ChevronsUpDown size={12} />
            </button>
          ) : (
            <span className={styles.sourceText}>{sourceName(state.sourceAppId)}</span>
          )}
        </div>
        <Equalizer playing={state.playing} color={accent} height={18} smooth />
        <IconButton label={t("music.idle.open")} size={32} onClick={() => invoke("music_idle_open").catch(() => {})}>
          <Maximize2 size={16} />
        </IconButton>
      </div>
      <ExpandedLyrics />
      <Scrubber />
      <div className={styles.transport}>
        <IconButton label={t("music.previous")} size={40} disabled={!state.canPrev} onClick={musicActions.prev}>
          <SkipBack size={20} {...filled} />
        </IconButton>
        <PlayPause size={48} iconSize={26} />
        <IconButton label={t("music.next")} size={40} disabled={!state.canNext} onClick={musicActions.next}>
          <SkipForward size={20} {...filled} />
        </IconButton>
      </div>
      <VolumeSlider tone={accent} />
    </div>
  );
}
