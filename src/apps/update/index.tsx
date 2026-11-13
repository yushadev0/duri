import { ArrowDownToLine } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { TextButton } from "../../components/Button";
import { DuriMark } from "../../components/DuriMark";
import { dismissUpdate, installUpdate, useUpdatePrompt, type UpdatePrompt } from "../../lib/updates";
import type { DuriApp } from "../types";
import styles from "./update.module.css";

/**
 * Asks about a new version right in the island, the way a Claude Code
 * permission prompt does: it bounces open and stays until answered with
 * Download or Not Now. (A Windows notification was easy to miss.) The
 * checks themselves live in src/lib/updates.ts.
 */

/** Just below a permission prompt (90): urgent things still come first. */
const PRIORITY = 89;

function Prompt() {
  const { t } = useTranslation();
  const status = useUpdatePrompt();
  if (!status) return null;
  const { version } = status;
  const downloading = status.kind === "downloading";
  const failed = status.kind === "failed";
  const percent = downloading && status.progress !== null ? Math.round(status.progress * 100) : null;

  return (
    <div className={styles.prompt} role="alert">
      <span className={styles.tile}>
        <DuriMark size={30} />
      </span>
      <div className={styles.meta}>
        <span className={styles.title}>
          {failed ? t("updates.installFailed", { version }) : t("updates.available", { version })}
        </span>
        <span className={styles.detail}>
          {downloading
            ? percent !== null
              ? t("updates.downloadingProgress", { percent })
              : t("updates.downloading")
            : failed
              ? t("updates.installFailedHint")
              : t("updates.promptHint")}
        </span>
      </div>
      {downloading ? (
        <div className={styles.progress} aria-hidden>
          <div className={styles.fill} style={{ width: `${percent ?? 8}%` }} />
        </div>
      ) : failed ? (
        <div className={styles.actions}>
          <TextButton variant="tinted" tone="var(--island-fg)" onClick={() => dismissUpdate(version)}>
            {t("updates.close")}
          </TextButton>
        </div>
      ) : (
        <div className={styles.actions}>
          <TextButton variant="tinted" tone="var(--island-fg)" onClick={() => dismissUpdate(version)}>
            {t("updates.notNow")}
          </TextButton>
          <TextButton variant="filled" tone="var(--accent-go)" onClick={() => installUpdate()}>
            {t("updates.download")}
          </TextButton>
        </div>
      )}
    </div>
  );
}

function Pill() {
  return (
    <div className={styles.pill}>
      <DuriMark size={22} />
      <ArrowDownToLine size={16} className={styles.pillIcon} />
    </div>
  );
}

function Minimal() {
  return <DuriMark size={24} />;
}

export const updateApp: DuriApp = {
  id: "update",
  nameKey: "apps.update.name",
  descriptionKey: "apps.update.description",
  Icon: ArrowDownToLine,
  accent: "var(--brand)",
  hidden: true,
  system: true,
  useActivity() {
    const status: UpdatePrompt | null = useUpdatePrompt();
    // Downloading keeps the same key: a new key would bounce the island again.
    const key = status ? `${status.version}:${status.kind === "failed" ? "failed" : "ask"}` : null;
    return useMemo(() => (key ? { priority: PRIORITY, alert: true, key } : null), [key]);
  },
  Pill,
  pillWidth: 150,
  pillCenter: false,
  Minimal,
  Compact: Prompt,
  compactSize: { w: 470, h: 72 },
  // Everything is in the prompt itself, so a click doesn't expand it.
  Expanded: Prompt,
  noExpand: true,
};
