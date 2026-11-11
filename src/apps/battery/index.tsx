import { Battery, BatteryCharging, BatteryFull, BatteryLow, BatteryMedium } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Ring } from "../../components/Ring";
import type { Activity, DuriApp } from "../types";
import {
  LOW_BATTERY,
  startPower,
  timeLeft,
  usePower,
  type BatteryNoticeKind,
  type BatteryState,
} from "../power/store";
import styles from "./battery.module.css";

/** Plugging in or out is a physical action, like the volume keys: it shows over everything. */
const PLUG: Activity = { priority: 94, hud: true };
const LOW: Activity = { priority: 92, hud: true };
/** Must not be missed: holds the island open. */
const CRITICAL: Activity = { priority: 99, hud: true, alert: true };

type Tone = "go" | "low" | "plain";

function toneOf(b: BatteryState): Tone {
  if (b.plugged) return "go";
  return b.percent <= LOW_BATTERY ? "low" : "plain";
}

/** The battery glyph for the level, with a bolt while charging. */
function BatteryIcon({ battery, size, color }: { battery: BatteryState; size: number; color?: string }) {
  const props = { size, color, "aria-hidden": true } as const;
  if (battery.charging) return <BatteryCharging {...props} />;
  if (battery.percent <= LOW_BATTERY) return <BatteryLow {...props} />;
  if (battery.percent <= 60) return <BatteryMedium {...props} />;
  return battery.percent >= 95 ? <BatteryFull {...props} /> : <Battery {...props} />;
}

const TONE_COLOR: Record<Tone, string> = {
  go: "var(--accent-go)",
  low: "var(--accent-stop)",
  plain: "var(--island-fg)",
};

function useText() {
  const { t } = useTranslation();
  return {
    /** The headline of a notice. */
    title(kind: BatteryNoticeKind | null, b: BatteryState) {
      if (kind === "critical") return t("battery.critical");
      if (kind === "low") return t("battery.low");
      if (b.plugged) return b.charging ? t("battery.charging") : t("battery.pluggedIn");
      return t("battery.onBattery");
    },
    /** One line of detail: fully charged, time left, or what to do. */
    detail(kind: BatteryNoticeKind | null, b: BatteryState) {
      const parts: string[] = [];
      if (kind === "low" || kind === "critical") parts.push(t("battery.plugIn"));
      if (b.plugged && !b.charging && b.percent >= 95) parts.push(t("battery.full"));
      const left = !b.plugged ? timeLeft(b.secondsLeft) : null;
      if (left) parts.push(left.h ? t("battery.leftHours", left) : t("battery.leftMinutes", left));
      if (b.saver) parts.push(t("battery.saver"));
      return parts.join(" · ");
    },
  };
}

function Percent({ battery }: { battery: BatteryState }) {
  const { t } = useTranslation();
  return (
    <span className={`${styles.percent} tabular`} data-low={!battery.plugged && battery.percent <= LOW_BATTERY}>
      {t("battery.percent", { level: battery.percent })}
    </span>
  );
}

function Pill() {
  const { battery, notice } = usePower();
  const text = useText();
  if (!notice) return null;
  const title = text.title(notice.kind, battery);
  return (
    <div className={styles.pill} role="status" aria-label={`${title}, ${battery.percent}%`}>
      <BatteryIcon battery={battery} size={20} color={TONE_COLOR[toneOf(battery)]} />
      <span className={styles.title}>{title}</span>
      <Percent battery={battery} />
    </div>
  );
}

function Minimal() {
  const { battery } = usePower();
  return <BatteryIcon battery={battery} size={16} color={TONE_COLOR[toneOf(battery)]} />;
}

/** The level as a ring, what's happening, and time left or what to do. */
function Panel() {
  const { t } = useTranslation();
  const { battery, notice } = usePower();
  const text = useText();
  const tone = toneOf(battery);
  const detail = text.detail(notice?.kind ?? null, battery);
  return (
    <div className={styles.panel} role="status">
      <Ring progress={battery.percent / 100} size={44} stroke={4} color={TONE_COLOR[tone === "plain" ? "go" : tone]}>
        <BatteryIcon battery={battery} size={18} />
      </Ring>
      <div className={styles.meta}>
        <span className={styles.label}>{t("apps.battery.name")}</span>
        <span className={styles.headline}>{text.title(notice?.kind ?? null, battery)}</span>
        {detail && <span className={styles.detail}>{detail}</span>}
      </div>
      <Percent battery={battery} />
    </div>
  );
}

export const batteryApp: DuriApp = {
  id: "battery",
  nameKey: "apps.battery.name",
  descriptionKey: "apps.battery.description",
  Icon: BatteryMedium,
  accent: "var(--accent-go)",
  hidden: true,
  start: startPower,
  useActivity() {
    const { notice, battery } = usePower();
    if (!notice || !battery.present) return null;
    const base = notice.kind === "critical" ? CRITICAL : notice.kind === "low" ? LOW : PLUG;
    return { ...base, key: `${notice.seq}` };
  },
  Pill,
  pillWidth: 250,
  pillCenter: false,
  Minimal,
  Compact: Panel,
  compactSize: { w: 380, h: 80 },
  Expanded: Panel,
  noExpand: true,
};
