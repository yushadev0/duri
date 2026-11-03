import { openUrl } from "@tauri-apps/plugin-opener";
import { Bell, BellOff, Check, FileText, Image as ImageIcon, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { IconButton, TextButton } from "../../components/Button";
import { useAgo } from "../../lib/useAgo";
import type { Activity, DuriApp } from "../types";
import { notificationActions, startNotifications, useNotifications, type Copied, type Notice } from "./store";
import styles from "./notifications.module.css";

const ACCENT = "var(--accent-notifications)";
/** A new notification sits just under the volume indicator and a finished timer. */
const NOTICE: Activity = { priority: 85 };
const COPIED: Activity = { priority: 93 };

function AppIcon({ notice, size }: { notice: Notice; size: number }) {
  return notice.icon ? (
    <img className={styles.icon} src={notice.icon} alt="" style={{ width: size, height: size }} />
  ) : (
    <span className={styles.iconFallback} style={{ width: size, height: size }} aria-hidden>
      <Bell size={Math.round(size * 0.55)} />
    </span>
  );
}

function CopiedText({ copied }: { copied: Copied }) {
  const { t } = useTranslation();
  if (copied.kind === "image") return <>{t("notifications.copiedImage")}</>;
  if (copied.kind === "files")
    return <>{copied.count > 1 ? t("notifications.copiedFiles", { count: copied.count }) : copied.preview}</>;
  return <>{copied.preview}</>;
}

function CopiedGlyph({ copied, size }: { copied: Copied; size: number }) {
  const Icon = copied.kind === "image" ? ImageIcon : copied.kind === "files" ? FileText : Check;
  return (
    <span className={styles.copiedGlyph} style={{ width: size, height: size }} aria-hidden>
      <Icon size={Math.round(size * 0.58)} strokeWidth={2.6} />
    </span>
  );
}

function Pill() {
  const { t } = useTranslation();
  const { flash } = useNotifications();
  if (!flash) return null;
  if (flash.kind === "copied")
    return (
      <div className={styles.pill} role="status">
        <CopiedGlyph copied={flash.copied} size={20} />
        <span className={styles.pillTitle}>{t("notifications.copied")}</span>
        <span className={styles.pillText}>
          <CopiedText copied={flash.copied} />
        </span>
      </div>
    );
  const n = flash.notice;
  return (
    <div className={styles.pill} role="status" aria-label={`${n.app}: ${n.title}`}>
      <AppIcon notice={n} size={20} />
      <span className={styles.pillTitle}>{n.title || n.app}</span>
      <span className={styles.pillText}>{n.title ? n.body : ""}</span>
    </div>
  );
}

function Minimal() {
  const { flash } = useNotifications();
  if (flash?.kind === "notice") return <AppIcon notice={flash.notice} size={22} />;
  if (flash?.kind === "copied") return <CopiedGlyph copied={flash.copied} size={22} />;
  return <Bell size={16} />;
}

function Compact() {
  const { t } = useTranslation();
  const { flash, items } = useNotifications();
  if (flash?.kind === "copied")
    return (
      <div className={styles.compact}>
        <CopiedGlyph copied={flash.copied} size={40} />
        <div className={styles.meta}>
          <span className={styles.title}>{t("notifications.copied")}</span>
          <span className={styles.body}>
            <CopiedText copied={flash.copied} />
          </span>
        </div>
      </div>
    );
  const n = flash?.kind === "notice" ? flash.notice : items[0];
  if (!n) return <Empty />;
  return (
    <div className={styles.compact}>
      <AppIcon notice={n} size={40} />
      <div className={styles.meta}>
        <span className={styles.app}>{n.app}</span>
        <span className={styles.title}>{n.title}</span>
        {n.body && <span className={styles.body}>{n.body}</span>}
      </div>
    </div>
  );
}

function Empty() {
  const { t } = useTranslation();
  const { access } = useNotifications();
  const denied = access === "denied";
  return (
    <div className={styles.empty}>
      {denied ? <BellOff size={28} color="var(--island-fg-3)" /> : <Bell size={28} color="var(--island-fg-3)" />}
      <p className={styles.emptyTitle}>{denied ? t("notifications.denied") : t("notifications.empty")}</p>
      <p className={styles.emptyHint}>
        {denied ? t("notifications.deniedHint") : access === "unavailable" ? t("notifications.unavailable") : t("notifications.emptyHint")}
      </p>
      {denied && (
        <TextButton variant="tinted" tone={ACCENT} onClick={() => openUrl("ms-settings:privacy-notifications").catch(() => {})}>
          {t("notifications.openSettings")}
        </TextButton>
      )}
    </div>
  );
}

function Row({ notice }: { notice: Notice }) {
  const { t } = useTranslation();
  const ago = useAgo();
  return (
    <li className={styles.row}>
      <AppIcon notice={notice} size={32} />
      <div className={styles.rowText}>
        <span className={styles.rowHead}>
          <span className={styles.rowApp}>{notice.app}</span>
          <span className={styles.ago}>{ago(notice.at)}</span>
        </span>
        <span className={styles.rowTitle}>{notice.title}</span>
        {notice.body && <span className={styles.rowBody}>{notice.body}</span>}
      </div>
      <span className={styles.rowAction}>
        <IconButton label={t("notifications.dismiss")} size={28} onClick={() => notificationActions.dismiss(notice.id)}>
          <X size={14} />
        </IconButton>
      </span>
    </li>
  );
}

function Expanded() {
  const { t } = useTranslation();
  const { items } = useNotifications();
  if (items.length === 0) return <Empty />;
  return (
    <div className={styles.expanded}>
      <div className={styles.toolbar}>
        <span className={styles.count}>{t("notifications.count", { count: items.length })}</span>
        <TextButton variant="tinted" tone="var(--island-fg)" onClick={notificationActions.clear}>
          {t("notifications.clear")}
        </TextButton>
      </div>
      <ul className={styles.list}>
        {items.map((n) => (
          <Row key={n.id} notice={n} />
        ))}
      </ul>
    </div>
  );
}

export const notificationsApp: DuriApp = {
  id: "notifications",
  nameKey: "apps.notifications.name",
  descriptionKey: "apps.notifications.description",
  Icon: Bell,
  accent: ACCENT,
  start: startNotifications,
  useActivity() {
    const { flash } = useNotifications();
    if (!flash) return null;
    return { ...(flash.kind === "copied" ? COPIED : NOTICE), key: flash.key };
  },
  Pill,
  pillWidth: 300,
  pillCenter: false,
  Minimal,
  Compact,
  compactSize: { w: 400, h: 76 },
  Expanded,
};
