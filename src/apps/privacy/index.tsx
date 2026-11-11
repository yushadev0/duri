import { Mic, ShieldCheck, Video } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../lib/settings";
import type { Activity, DuriApp } from "../types";
import { startPrivacy, usePrivacy, watched, type Device } from "./store";
import styles from "./privacy.module.css";

/** Like a new notification, just above it. */
const NOTICE: Activity = { priority: 87 };

/**
 * A device's mark: its own icon in its own color (green camera, orange
 * microphone, as on iPhone), so the two differ by shape, not color alone.
 */
export function DeviceBadge({ device, size = 22 }: { device: Device; size?: number }) {
  const Icon = device === "camera" ? Video : Mic;
  return (
    <span className={styles.badge} data-device={device} style={{ width: size, height: size }} aria-hidden>
      <Icon size={Math.round(size * 0.55)} strokeWidth={2.4} />
    </span>
  );
}

function Pill() {
  const { t } = useTranslation();
  const { notice } = usePrivacy();
  if (!notice) return null;
  const what = t(`privacy.${notice.device}`);
  return (
    <div className={styles.pill} role="status" aria-label={t("privacy.started", { app: notice.name, device: what })}>
      <DeviceBadge device={notice.device} />
      <span className={styles.name}>{notice.name}</span>
      <span className={styles.device}>{what}</span>
    </div>
  );
}

function Minimal() {
  const { notice } = usePrivacy();
  return <DeviceBadge device={notice?.device ?? "camera"} size={24} />;
}

/** Every app using a watched device right now. */
function Panel() {
  const { t } = useTranslation();
  const settings = useSettings();
  const { inUse } = usePrivacy();
  const rows = (["camera", "microphone"] as const)
    .filter((d) => watched(d, settings))
    .flatMap((device) => inUse[device].map((name) => ({ device, name })));
  if (!rows.length) {
    return (
      <div className={styles.empty}>
        <ShieldCheck size={18} aria-hidden />
        <span>{t("privacy.none")}</span>
      </div>
    );
  }
  return (
    <ul className={styles.list} aria-label={t("apps.privacy.name")}>
      {rows.map((r) => (
        <li key={`${r.device}:${r.name}`} className={styles.row}>
          <DeviceBadge device={r.device} size={26} />
          <span className={styles.name}>{r.name}</span>
          <span className={styles.device}>{t(`privacy.using.${r.device}`)}</span>
        </li>
      ))}
    </ul>
  );
}

export const privacyApp: DuriApp = {
  id: "privacy",
  nameKey: "apps.privacy.name",
  descriptionKey: "apps.privacy.description",
  Icon: Video,
  accent: "var(--accent-go)",
  hidden: true,
  start: startPrivacy,
  useActivity() {
    const { notice } = usePrivacy();
    return notice ? { ...NOTICE, key: `${notice.seq}` } : null;
  },
  Pill,
  pillWidth: 280,
  pillCenter: false,
  Minimal,
  Compact: Panel,
  compactSize: { w: 340, h: 76 },
  Expanded: Panel,
  noExpand: true,
};
