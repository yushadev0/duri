import { Volume2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { VolumeIcon, VolumeSlider } from "../../components/VolumeSlider";
import type { Activity, DuriApp } from "../types";
import { startVolume, useVolume } from "./store";
import styles from "./volume.module.css";

/** Above everything except a finished timer, but only for a moment. */
const HUD: Activity = { priority: 95 };

function Pill() {
  const { t } = useTranslation();
  const { state } = useVolume();
  const level = state.muted ? 0 : state.level;
  return (
    <div className={styles.pill} role="status" aria-label={`${t("volume.level")} ${Math.round(level * 100)}`}>
      <VolumeIcon level={state.level} muted={state.muted} size={17} />
      <span className={styles.bar} aria-hidden>
        <span className={styles.barFill} style={{ width: `${level * 100}%` }} />
      </span>
      <span className={`${styles.percent} tabular`}>{state.muted ? t("volume.muted") : Math.round(level * 100)}</span>
    </div>
  );
}

function Minimal() {
  const { state } = useVolume();
  return <VolumeIcon level={state.level} muted={state.muted} size={16} />;
}

function Panel() {
  return (
    <div className={styles.panel}>
      <VolumeSlider />
    </div>
  );
}

export const volumeApp: DuriApp = {
  id: "volume",
  nameKey: "apps.volume.name",
  descriptionKey: "apps.volume.description",
  Icon: Volume2,
  accent: "var(--island-fg)",
  hidden: true,
  start: startVolume,
  useActivity() {
    const { hud } = useVolume();
    return hud ? HUD : null;
  },
  Pill,
  pillWidth: 230,
  Minimal,
  Compact: Panel,
  compactSize: { w: 340, h: 60 },
  Expanded: Panel,
  expandedSize: { w: 420, h: 120 },
};
