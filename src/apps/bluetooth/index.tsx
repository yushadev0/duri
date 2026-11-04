import {
  Bluetooth,
  Gamepad2,
  Headphones,
  Keyboard,
  Laptop,
  Mouse,
  Smartphone,
  Speaker,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useAgo } from "../../lib/useAgo";
import type { Activity, DuriApp } from "../types";
import { startBluetooth, useBluetooth, type DeviceEvent, type DeviceKind } from "./store";
import styles from "./bluetooth.module.css";

/** Like a new notification. */
const NOTICE: Activity = { priority: 85 };

const ICONS: Record<DeviceKind, LucideIcon> = {
  headphones: Headphones,
  speaker: Speaker,
  phone: Smartphone,
  computer: Laptop,
  keyboard: Keyboard,
  mouse: Mouse,
  gamepad: Gamepad2,
  other: Bluetooth,
};

function Glyph({ device, size }: { device: DeviceEvent; size: number }) {
  const Icon = ICONS[device.kind] ?? Bluetooth;
  return (
    <span className={styles.glyph} data-connected={device.connected} style={{ width: size, height: size }} aria-hidden>
      <Icon size={Math.round(size * 0.55)} strokeWidth={2.2} />
    </span>
  );
}

function useStatus() {
  const { t } = useTranslation();
  return (d: DeviceEvent) => (d.connected ? t("bluetooth.connected") : t("bluetooth.disconnected"));
}

function Pill() {
  const { notice } = useBluetooth();
  const status = useStatus();
  if (!notice) return null;
  return (
    <div className={styles.pill} role="status" aria-label={`${notice.name}: ${status(notice)}`}>
      <Glyph device={notice} size={22} />
      <span className={styles.name}>{notice.name}</span>
      <span className={styles.status}>{status(notice)}</span>
    </div>
  );
}

function Minimal() {
  const { notice } = useBluetooth();
  return notice ? <Glyph device={notice} size={24} /> : <Bluetooth size={16} />;
}

function Compact() {
  const { t } = useTranslation();
  const { notice, recent } = useBluetooth();
  const status = useStatus();
  const d = notice ?? recent[0];
  if (!d) return null;
  return (
    <div className={styles.compact}>
      <Glyph device={d} size={40} />
      <div className={styles.meta}>
        <span className={styles.label}>{t("apps.bluetooth.name")}</span>
        <span className={styles.title}>{d.name}</span>
        <span className={styles.body}>{status(d)}</span>
      </div>
    </div>
  );
}

/** The last few changes this session. */
function Expanded() {
  const { recent } = useBluetooth();
  const status = useStatus();
  const ago = useAgo();
  return (
    <ul className={styles.list}>
      {recent.map((d) => (
        <li key={`${d.at}${d.name}`} className={styles.row}>
          <Glyph device={d} size={32} />
          <div className={styles.meta}>
            <span className={styles.title}>{d.name}</span>
            <span className={styles.body}>{status(d)}</span>
          </div>
          <span className={styles.ago}>{ago(d.at)}</span>
        </li>
      ))}
    </ul>
  );
}

export const bluetoothApp: DuriApp = {
  id: "bluetooth",
  nameKey: "apps.bluetooth.name",
  descriptionKey: "apps.bluetooth.description",
  Icon: Bluetooth,
  accent: "var(--accent-bluetooth)",
  hidden: true,
  start: startBluetooth,
  useActivity() {
    const { notice } = useBluetooth();
    return notice ? { ...NOTICE, key: `${notice.seq}` } : null;
  },
  Pill,
  pillWidth: 290,
  pillCenter: false,
  Minimal,
  Compact,
  compactSize: { w: 380, h: 76 },
  Expanded,
  expandedSize: { w: 400, h: 220 },
};
