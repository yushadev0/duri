import { Sun, SunDim, SunMedium } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import sliderStyles from "../../components/VolumeSlider.module.css";
import volumeStyles from "../volume/volume.module.css";
import { setBrightness, startPower, usePower } from "../power/store";
import type { Activity, DuriApp } from "../types";

/** Beside the volume indicator: shows over everything, even fullscreen. */
const HUD: Activity = { priority: 95, hud: true };

function BrightnessIcon({ level, size = 16 }: { level: number; size?: number }) {
  if (level < 34) return <SunDim size={size} aria-hidden />;
  return level < 67 ? <SunMedium size={size} aria-hidden /> : <Sun size={size} aria-hidden />;
}

/** Same shape as the volume indicator: icon, bar, number. */
function Pill() {
  const { t } = useTranslation();
  const { brightness } = usePower();
  const level = brightness ?? 0;
  return (
    <div className={volumeStyles.pill} role="status" aria-label={`${t("brightness.level")} ${level}`}>
      <BrightnessIcon level={level} size={17} />
      <span className={volumeStyles.bar} aria-hidden>
        <span className={volumeStyles.barFill} style={{ width: `${level}%` }} />
      </span>
      <span className={`${volumeStyles.percent} tabular`}>{level}</span>
    </div>
  );
}

function Minimal() {
  const { brightness } = usePower();
  return <BrightnessIcon level={brightness ?? 100} />;
}

function Slider() {
  const { t } = useTranslation();
  const { brightness } = usePower();
  const [drag, setDrag] = useState<number | null>(null);
  const track = useRef<HTMLDivElement>(null);
  if (brightness === null) return null;

  const level = drag ?? brightness;
  const toLevel = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * 100);
  };
  const apply = (l: number) => {
    setDrag(l);
    setBrightness(l);
  };

  return (
    <div className={sliderStyles.row}>
      <span style={{ width: 28, display: "grid", placeItems: "center" }}>
        <BrightnessIcon level={level} />
      </span>
      <div
        ref={track}
        className={sliderStyles.track}
        role="slider"
        tabIndex={0}
        aria-label={t("brightness.level")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={level}
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
          if (e.key === "ArrowRight" || e.key === "ArrowUp") setBrightness(brightness + 5);
          if (e.key === "ArrowLeft" || e.key === "ArrowDown") setBrightness(brightness - 5);
        }}
      >
        <div className={sliderStyles.rail}>
          <div className={sliderStyles.fill} style={{ width: `${level}%`, background: "var(--island-fg)" }} />
        </div>
      </div>
      <span className={`${sliderStyles.value} tabular`}>{level}</span>
    </div>
  );
}

function Panel() {
  return (
    <div className={volumeStyles.panel}>
      <div style={{ flex: 1 }}>
        <Slider />
      </div>
    </div>
  );
}

export const brightnessApp: DuriApp = {
  id: "brightness",
  nameKey: "apps.brightness.name",
  descriptionKey: "apps.brightness.description",
  Icon: Sun,
  accent: "var(--accent-notes)",
  hidden: true,
  start: startPower,
  useActivity() {
    const { brightnessHud } = usePower();
    return brightnessHud ? HUD : null;
  },
  Pill,
  pillWidth: 230,
  pillCenter: false,
  Minimal,
  Compact: Panel,
  compactSize: { w: 340, h: 60 },
  // A slider is all there is; a click doesn't open a bigger view.
  Expanded: Panel,
  noExpand: true,
};
