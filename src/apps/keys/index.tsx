import { ArrowBigUpDash, Keyboard, ScrollText, TextCursorInput, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Activity, DuriApp } from "../types";
import { LOCK_KEYS, startKeys, useKeys, type LockKey } from "./store";
import styles from "./keys.module.css";

/** Next to the volume indicator: shows over everything, even fullscreen. */
const HUD: Activity = { priority: 96, hud: true };

const ICONS: Record<LockKey, LucideIcon> = {
  caps: ArrowBigUpDash,
  insert: TextCursorInput,
  scroll: ScrollText,
};

function Pill() {
  const { t } = useTranslation();
  const { hud } = useKeys();
  if (!hud) return null;
  const Icon = ICONS[hud.key];
  const name = t(`keys.${hud.key}`);
  const state = hud.on ? t("keys.on") : t("keys.off");
  return (
    <div className={styles.pill} role="status" aria-label={`${name}: ${state}`}>
      <Icon size={17} strokeWidth={hud.on ? 2.4 : 1.8} aria-hidden />
      <span className={styles.name}>{name}</span>
      <span className={styles.state} data-on={hud.on}>
        {state}
      </span>
    </div>
  );
}

function Minimal() {
  const { hud } = useKeys();
  const Icon = hud ? ICONS[hud.key] : Keyboard;
  return <Icon size={16} strokeWidth={hud?.on ? 2.4 : 1.8} />;
}

/** All three keys at once, filled when on. */
function Panel() {
  const { t } = useTranslation();
  const { states } = useKeys();
  return (
    <div className={styles.panel}>
      {LOCK_KEYS.map((key) => {
        const Icon = ICONS[key];
        const on = states[key];
        return (
          <div key={key} className={styles.key} data-on={on}>
            <Icon size={16} aria-hidden />
            <span className={styles.keyText}>
              <span className={styles.keyName}>{t(`keys.${key}`)}</span>
              <span className={styles.keyState}>{on ? t("keys.on") : t("keys.off")}</span>
            </span>
          </div>
        );
      })}
    </div>
  );
}

export const keysApp: DuriApp = {
  id: "keys",
  nameKey: "apps.keys.name",
  descriptionKey: "apps.keys.description",
  Icon: Keyboard,
  accent: "var(--island-fg)",
  hidden: true,
  start: startKeys,
  useActivity() {
    const { hud } = useKeys();
    return hud ? { ...HUD, key: `${hud.seq}` } : null;
  },
  Pill,
  pillWidth: 200,
  pillCenter: false,
  Minimal,
  Compact: Panel,
  compactSize: { w: 420, h: 72 },
  // Nothing to do here beyond reading the state, so a click doesn't expand it.
  Expanded: Panel,
  noExpand: true,
};
