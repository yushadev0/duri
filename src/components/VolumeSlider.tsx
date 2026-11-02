import { Volume1, Volume2, VolumeX } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { volumeActions, useVolume } from "../apps/volume/store";
import { IconButton } from "./Button";
import styles from "./VolumeSlider.module.css";

export function VolumeIcon({ level, muted, size = 16 }: { level: number; muted: boolean; size?: number }) {
  if (muted || level === 0) return <VolumeX size={size} />;
  return level < 0.5 ? <Volume1 size={size} /> : <Volume2 size={size} />;
}

/** System output volume: mute button, draggable bar, percentage. */
export function VolumeSlider({ tone = "var(--island-fg)" }: { tone?: string }) {
  const { t } = useTranslation();
  const { state } = useVolume();
  const [drag, setDrag] = useState<number | null>(null);
  const track = useRef<HTMLDivElement>(null);
  if (!state.available) return null;

  const level = drag ?? (state.muted ? 0 : state.level);
  const toLevel = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * 100) / 100;
  };
  const apply = (l: number) => {
    setDrag(l);
    volumeActions.set(l);
  };

  return (
    <div className={styles.row}>
      <IconButton
        label={state.muted ? t("volume.unmute") : t("volume.mute")}
        size={28}
        pressed={state.muted}
        onClick={volumeActions.toggleMute}
      >
        <VolumeIcon level={state.level} muted={state.muted} />
      </IconButton>
      <div
        ref={track}
        className={styles.track}
        role="slider"
        tabIndex={0}
        aria-label={t("volume.level")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(level * 100)}
        data-dragging={drag !== null}
        onClick={(e) => e.stopPropagation()}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          apply(toLevel(e.clientX));
        }}
        onPointerMove={(e) => drag !== null && apply(toLevel(e.clientX))}
        onPointerUp={() => setDrag(null)}
        onPointerCancel={() => setDrag(null)}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight" || e.key === "ArrowUp") volumeActions.set(Math.min(1, state.level + 0.05));
          if (e.key === "ArrowLeft" || e.key === "ArrowDown") volumeActions.set(Math.max(0, state.level - 0.05));
        }}
      >
        <div className={styles.rail}>
          <div className={styles.fill} style={{ width: `${level * 100}%`, background: tone }} />
        </div>
      </div>
      <span className={`${styles.value} tabular`}>{Math.round(level * 100)}</span>
    </div>
  );
}
