import { invoke } from "@tauri-apps/api/core";
import { AudioLines, Disc3, MicVocal, Pause, Play, SkipBack, SkipForward, Waypoints, X } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { IconButton } from "../../../components/Button";
import { formatClock, useHour12 } from "../../../lib/clock";
import { useAlignedNow } from "../../../lib/ticks";
import { updateSettings, useSettings, type MusicIdleStyle } from "../../../lib/settings";
import { useLyrics, type LyricsStatus, type Timeline } from "../lyrics";
import { musicActions, startMusic, useMusic } from "../store";
import styles from "./idle.module.css";
import { IdleConstellation } from "./IdleConstellation";
import { CoverView, LyricsView, NowPlaying } from "./IdleViews";
import { useAudioMeter } from "./meter";

type Variant = Exclude<MusicIdleStyle, "auto">;

const VARIANTS: { id: Variant; Icon: typeof Waypoints }[] = [
  { id: "network", Icon: Waypoints },
  { id: "reactive", Icon: AudioLines },
  { id: "lyrics", Icon: MicVocal },
  { id: "cover", Icon: Disc3 },
];

/** Controls fade away after this long without the pointer moving. */
const CHROME_MS = 2600;

const filled = { fill: "currentColor", strokeWidth: 1.5 } as const;

/** "auto" is the network, or the calm lyrics when the system asks for less motion. */
export function resolveVariant(style: MusicIdleStyle, reduceMotion: boolean): Variant {
  // Anything else (a look that was removed) counts as "auto".
  if (VARIANTS.some((v) => v.id === style)) return style as Variant;
  return reduceMotion ? "lyrics" : "network";
}

/** How closely the lyrics follow the song, for the note under the credit. */
function syncKey(timeline: Timeline): string {
  if (!timeline.synced) return "music.idle.sync.unsyncedShort";
  return timeline.wordTimed ? "music.idle.sync.wordSynced" : "music.idle.sync.lineSynced";
}

const close = () => invoke("music_idle_close").catch(() => {});

/** Shows the controls on pointer movement or a key press, hides them when idle. */
function useChrome() {
  const [visible, setVisible] = useState(true);
  const hovering = useRef(false);
  useEffect(() => {
    let id: ReturnType<typeof setTimeout>;
    const arm = () => {
      clearTimeout(id);
      id = setTimeout(() => !hovering.current && setVisible(false), CHROME_MS);
    };
    const show = () => {
      setVisible(true);
      arm();
    };
    arm();
    window.addEventListener("pointermove", show);
    window.addEventListener("keydown", show);
    return () => {
      clearTimeout(id);
      window.removeEventListener("pointermove", show);
      window.removeEventListener("keydown", show);
    };
  }, []);
  const hoverProps = {
    onPointerEnter: () => (hovering.current = true),
    onPointerLeave: () => (hovering.current = false),
  };
  return { visible, hoverProps };
}

/** Why the lyrics look shows the cover instead. */
function lyricsNote(status: LyricsStatus, t: (k: string) => string): string | undefined {
  switch (status.kind) {
    case "loading":
      return t("music.idle.searching");
    case "none":
      return t("music.idle.noLyrics");
    case "instrumental":
      return t("music.idle.instrumental");
    case "error":
      return t("music.idle.error");
    default:
      return undefined;
  }
}

/** A quiet clock at the top of Idle Mode (Settings › Apps › Music). */
function IdleClock() {
  const { i18n } = useTranslation();
  const hour12 = useHour12();
  const now = new Date(useAlignedNow(true, 60_000));
  return <time className={`${styles.clock} tabular`}>{formatClock(now, i18n.language, hour12)}</time>;
}

/** The fullscreen Idle Mode window (label "idle"), opened from the Music app. */
export function MusicIdle() {
  const { t } = useTranslation();
  const { state, accent } = useMusic();
  const settings = useSettings();
  const reduceMotion = useReducedMotion() ?? false;
  const variant = resolveVariant(settings.musicIdleStyle, reduceMotion);
  const lyrics = useLyrics(state, variant === "network" || variant === "lyrics");
  const { visible, hoverProps } = useChrome();
  // The sound-reactive circle lives in the network and reactive looks.
  useAudioMeter((variant === "network" || variant === "reactive") && state.available);

  useEffect(() => {
    startMusic();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      else if (e.key === " " && !(e.target instanceof HTMLButtonElement)) {
        e.preventDefault();
        musicActions.toggle();
      } else if (/^[1-9]$/.test(e.key) && Number(e.key) <= VARIANTS.length) {
        updateSettings({ musicIdleStyle: VARIANTS[Number(e.key) - 1].id });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const timeline = lyrics.kind === "ready" ? lyrics.timeline : null;
  let stage;
  if (!state.available) {
    stage = (
      <div className={styles.empty}>
        <h1 className={styles.coverTitle}>{t("music.nothingPlaying")}</h1>
        <p className={styles.coverArtist}>{t("music.nothingPlayingHint")}</p>
      </div>
    );
  } else if (variant === "reactive" || (variant === "network" && !timeline)) {
    // Just the sound: also the network's look while there are no lyrics to
    // draw (searching, instrumental, none found, offline).
    stage = (
      <>
        <IdleConstellation
          timeline={null}
          accent={accent}
          reduceMotion={reduceMotion}
          size={variant === "reactive" ? 1.35 : 1}
          backdrop={variant === "reactive"}
        />
        <NowPlaying />
      </>
    );
  } else if (variant === "cover" || !timeline) {
    stage = <CoverView note={variant === "cover" ? undefined : lyricsNote(lyrics, t)} />;
  } else if (variant === "lyrics" || (variant === "network" && !timeline.synced)) {
    // Untimed lyrics can't be drawn word by word: they show as a page.
    stage = <LyricsView timeline={timeline} reduceMotion={reduceMotion} />;
  } else {
    stage = (
      <>
        <IdleConstellation timeline={timeline} accent={accent} reduceMotion={reduceMotion} />
        <NowPlaying />
      </>
    );
  }

  const backdrop = state.available
    ? `radial-gradient(120% 120% at 12% 0%, color-mix(in srgb, ${accent} 42%, black) 0%, color-mix(in srgb, ${accent} 14%, black) 45%, #000 82%)`
    : "#000";

  return (
    <main
      className={styles.root}
      data-chrome={visible}
      style={{ "--accent": accent, background: backdrop } as CSSProperties}
    >
      {stage}
      {settings.musicIdleClock && <IdleClock />}

      <header className={`${styles.chrome} ${styles.top}`} {...hoverProps}>
        <div className={styles.segmented} role="radiogroup" aria-label={t("music.idle.style")}>
          {VARIANTS.map(({ id, Icon }) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={variant === id}
              className={styles.segment}
              onClick={() => updateSettings({ musicIdleStyle: id })}
            >
              <Icon size={16} />
              {t(`music.idle.styles.${id}`)}
            </button>
          ))}
        </div>
        <IconButton label={t("music.idle.exit")} size={40} onClick={close}>
          <X size={20} />
        </IconButton>
      </header>

      {state.available && (
        <footer className={`${styles.chrome} ${styles.bottom}`} {...hoverProps}>
          <IconButton label={t("music.previous")} size={44} disabled={!state.canPrev} onClick={musicActions.prev}>
            <SkipBack size={20} {...filled} />
          </IconButton>
          <IconButton
            label={state.playing ? t("music.pause") : t("music.play")}
            size={56}
            disabled={!state.canToggle}
            onClick={musicActions.toggle}
          >
            {state.playing ? <Pause size={26} {...filled} /> : <Play size={26} {...filled} />}
          </IconButton>
          <IconButton label={t("music.next")} size={44} disabled={!state.canNext} onClick={musicActions.next}>
            <SkipForward size={20} {...filled} />
          </IconButton>
          {(variant === "network" || variant === "lyrics") && timeline && (
            <span className={styles.credit}>
              <span>{t("music.idle.credit")}</span>
              <span className={styles.creditSync}>{t(syncKey(timeline))}</span>
            </span>
          )}
        </footer>
      )}
    </main>
  );
}
