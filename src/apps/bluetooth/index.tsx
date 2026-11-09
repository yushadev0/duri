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
import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { Ring } from "../../components/Ring";
import type { Activity, DuriApp } from "../types";
import {
  batteryOf,
  LOW_BATTERY,
  startBluetooth,
  useBluetooth,
  type ConnectedDevice,
  type DeviceEvent,
  type DeviceKind,
  type Notice,
} from "./store";
import styles from "./bluetooth.module.css";

/** Like a new notification. */
const NOTICE: Activity = { priority: 85 };
/** Devices shown side by side in the hover view. */
const COMPACT_MAX = 3;
/** Tile width by column count: fewer devices get room for longer names. */
const TILE: Record<number, number> = { 1: 200, 2: 160, 3: 124, 4: 96 };

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

function Glyph({ device, size, low }: { device: DeviceEvent; size: number; low?: boolean }) {
  const Icon = ICONS[device.kind] ?? Bluetooth;
  return (
    <span className={styles.glyph} data-connected={device.connected} data-low={low} style={{ width: size, height: size }} aria-hidden>
      <Icon size={Math.round(size * 0.55)} strokeWidth={2.2} />
    </span>
  );
}

const isLow = (battery: number | null) => battery !== null && battery <= LOW_BATTERY;

/** The device icon inside a ring that is as full as the battery. */
function BatteryRing({ device, size }: { device: ConnectedDevice; size: number }) {
  const Icon = ICONS[device.kind] ?? Bluetooth;
  const color = device.battery === null ? "var(--island-fg-3)" : isLow(device.battery) ? "var(--accent-stop)" : "var(--accent-go)";
  return (
    <Ring progress={(device.battery ?? 0) / 100} size={size} stroke={size >= 56 ? 4 : 3} color={color}>
      <Icon size={Math.round(size * 0.4)} strokeWidth={2} aria-hidden />
    </Ring>
  );
}

function Percent({ battery }: { battery: number | null }) {
  const { t } = useTranslation();
  return (
    <span className={`${styles.percent} tabular`} data-low={isLow(battery)} data-unknown={battery === null}>
      {battery === null ? t("bluetooth.unknown") : t("bluetooth.percent", { level: battery })}
    </span>
  );
}

function useLabels() {
  const { t } = useTranslation();
  return {
    status: (n: Notice) =>
      n.low !== undefined ? t("bluetooth.lowNotice") : n.connected ? t("bluetooth.connected") : t("bluetooth.disconnected"),
    /** What a screen reader says for one connected device. */
    device: (d: ConnectedDevice) =>
      d.battery === null
        ? `${d.name}: ${t("bluetooth.noBattery")}`
        : `${d.name}: ${t("bluetooth.battery", { level: d.battery })}${isLow(d.battery) ? `, ${t("bluetooth.low")}` : ""}`,
  };
}

/** The level a notice shows: the low reading, or the live one once connected. */
function noticeBattery(notice: Notice, devices: ConnectedDevice[]) {
  if (notice.low !== undefined) return notice.low;
  return notice.connected ? batteryOf(devices, notice.id) : null;
}

/** Connected, disconnected or battery low, with the level when known. */
function Pill() {
  const { t } = useTranslation();
  const { notice, devices } = useBluetooth();
  const labels = useLabels();
  if (!notice) return null;
  const battery = noticeBattery(notice, devices);
  return (
    <div
      className={styles.pill}
      role="status"
      aria-label={`${notice.name}: ${labels.status(notice)}${battery === null ? "" : `, ${t("bluetooth.battery", { level: battery })}`}`}
    >
      <Glyph device={notice} size={22} low={notice.low !== undefined} />
      <span className={styles.name}>{notice.name}</span>
      <span className={styles.status}>{labels.status(notice)}</span>
      {battery !== null && <Percent battery={battery} />}
    </div>
  );
}

function Minimal() {
  const { notice } = useBluetooth();
  return notice ? <Glyph device={notice} size={24} low={notice.low !== undefined} /> : <Bluetooth size={16} />;
}

function Empty({ large }: { large?: boolean }) {
  const { t } = useTranslation();
  return (
    <div className={large ? styles.empty : styles.emptyCompact}>
      <span className={styles.emptyIcon} aria-hidden>
        <Bluetooth size={large ? 22 : 18} />
      </span>
      <div className={styles.emptyText}>
        <span className={styles.title}>{t("bluetooth.none")}</span>
        <span className={styles.body}>{t("bluetooth.noneHint")}</span>
      </div>
    </div>
  );
}

/**
 * Hover: the notice while it lasts, otherwise the connected devices side by
 * side with their battery.
 */
function Compact() {
  const { t } = useTranslation();
  const { notice, devices } = useBluetooth();
  const labels = useLabels();
  if (notice) {
    const battery = noticeBattery(notice, devices);
    return (
      <div className={styles.compact}>
        <Glyph device={notice} size={40} low={notice.low !== undefined} />
        <div className={styles.meta}>
          <span className={styles.label}>{t("apps.bluetooth.name")}</span>
          <span className={styles.title}>{notice.name}</span>
          <span className={styles.body}>{labels.status(notice)}</span>
        </div>
        {battery !== null && <Percent battery={battery} />}
      </div>
    );
  }
  if (!devices.length) return <Empty />;
  const shown = devices.slice(0, COMPACT_MAX);
  const more = devices.length - shown.length;
  return (
    <ul className={styles.cells} aria-label={t("bluetooth.devices")}>
      {shown.map((d) => (
        <li key={d.id} className={styles.cell} aria-label={labels.device(d)}>
          <BatteryRing device={d} size={40} />
          <div className={styles.meta} aria-hidden>
            <Percent battery={d.battery} />
            <span className={styles.cellName}>{d.name}</span>
          </div>
        </li>
      ))}
      {more > 0 && <li className={styles.more}>{t("bluetooth.more", { count: more })}</li>}
    </ul>
  );
}

/** Every connected device as a battery ring, four to a row. */
function Expanded() {
  const { t } = useTranslation();
  const { devices } = useBluetooth();
  const labels = useLabels();
  if (!devices.length) return <Empty large />;
  const cols = Math.min(devices.length, 4);
  return (
    <ul
      className={styles.grid}
      style={{ "--cols": cols, "--tile": `${TILE[cols]}px` } as CSSProperties}
      aria-label={t("bluetooth.devices")}
    >
      {devices.map((d) => (
        <li key={d.id} className={styles.tile} aria-label={labels.device(d)} title={d.name}>
          <BatteryRing device={d} size={64} />
          <Percent battery={d.battery} />
          <span className={styles.tileName} aria-hidden>
            {d.name}
          </span>
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
  start: startBluetooth,
  useActivity() {
    const { notice } = useBluetooth();
    return notice ? { ...NOTICE, key: `${notice.seq}` } : null;
  },
  Pill,
  pillWidth: 300,
  pillCenter: false,
  Minimal,
  Compact,
  compactSize: { w: 400, h: 76 },
  Expanded,
};
