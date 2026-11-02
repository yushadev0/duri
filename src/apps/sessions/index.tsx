import { Asterisk, Bot, Check, SquareTerminal, X } from "lucide-react";
import { useMemo, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { IconButton, TextButton } from "../../components/Button";
import { useAgo } from "../../lib/useAgo";
import { useNow } from "../../lib/useNow";
import type { Activity, DuriApp } from "../types";
import {
  DONE_VISIBLE_MS,
  focusSession,
  needsYou,
  sessionActions,
  startSessions,
  useHookStatus,
  useSessions,
  type Agent,
  type Session,
  type Status,
} from "./store";
import styles from "./sessions.module.css";

const ACCENT = "var(--accent-sessions)";
const STATUS_COLOR: Record<Status, string> = {
  working: "var(--status-working)",
  waiting: "var(--accent-timer)",
  done: "var(--accent-go)",
  idle: "var(--island-fg-3)",
};

function AgentIcon({ agent, size = 18 }: { agent: Agent; size?: number }) {
  return agent === "claude" ? (
    <Asterisk size={size} color="var(--agent-claude)" strokeWidth={2.6} aria-label="Claude Code" />
  ) : (
    <SquareTerminal size={size} color="var(--island-fg)" aria-label="Codex" />
  );
}

function StatusGlyph({ status }: { status: Status }) {
  const color = STATUS_COLOR[status];
  if (status === "working")
    return (
      <span className={styles.dots} style={{ color }} aria-hidden>
        <i />
        <i />
        <i />
      </span>
    );
  if (status === "waiting")
    return (
      <span className={styles.bang} style={{ background: color }} aria-hidden>
        !
      </span>
    );
  if (status === "done") return <Check size={16} color={color} strokeWidth={3} aria-hidden />;
  return <span className={styles.idleDot} aria-hidden />;
}

function StatusChip({ status }: { status: Status }) {
  const { t } = useTranslation();
  return (
    <span className={styles.chip} style={{ "--tone": STATUS_COLOR[status] } as CSSProperties}>
      {t(`sessions.status.${status}`)}
    </span>
  );
}

function useFocus() {
  const { sessions, acknowledged } = useSessions();
  return { session: focusSession(sessions, acknowledged), sessions, acknowledged };
}

export function SessionsPill() {
  const { session, sessions } = useFocus();
  if (!session) return null;
  const others = sessions.filter((s) => s.status === "working" || s.status === "waiting").length - 1;
  return (
    <div className={styles.pill}>
      <AgentIcon agent={session.agent} />
      <span className={styles.pillRight}>
        {others > 0 && <span className={styles.count}>+{others}</span>}
        <StatusGlyph status={session.status} />
      </span>
    </div>
  );
}

export function SessionsMinimal() {
  const { session } = useFocus();
  if (!session) return null;
  return (
    <span className={styles.minimal} style={{ borderColor: STATUS_COLOR[session.status] }}>
      <AgentIcon agent={session.agent} size={16} />
    </span>
  );
}

export function SessionsCompact() {
  const { t } = useTranslation();
  const { session, acknowledged } = useFocus();
  if (!session) return null;
  const alert = needsYou(session, acknowledged);
  return (
    <div className={styles.compact} role={alert ? "alert" : undefined}>
      <span className={styles.tile}>
        <AgentIcon agent={session.agent} size={22} />
      </span>
      <div className={styles.meta}>
        <span className={styles.project}>{session.project || t(`sessions.agent.${session.agent}`)}</span>
        <span className={styles.detail}>{session.detail ?? t(`sessions.agent.${session.agent}`)}</span>
      </div>
      {alert ? (
        <TextButton variant="filled" tone={STATUS_COLOR.waiting} onClick={() => sessionActions.acknowledge(session)}>
          {t("sessions.acknowledge")}
        </TextButton>
      ) : (
        <StatusChip status={session.status} />
      )}
    </div>
  );
}

function Empty() {
  const { t } = useTranslation();
  const hooks = useHookStatus();
  const [error, setError] = useState<string | null>(null);
  const connect = () => sessionActions.installHooks().catch((e) => setError(String(e)));
  return (
    <div className={styles.empty}>
      <Bot size={28} color="var(--island-fg-3)" />
      <p className={styles.emptyTitle}>{t("sessions.empty")}</p>
      {hooks && !hooks.installed ? (
        <>
          <p className={styles.emptyHint}>{t("sessions.connectHint")}</p>
          <TextButton variant="filled" tone={ACCENT} onClick={connect}>
            {t("sessions.connect")}
          </TextButton>
        </>
      ) : (
        <p className={styles.emptyHint}>{t("sessions.emptyHint")}</p>
      )}
      {error && <p className={styles.error}>{error}</p>}
    </div>
  );
}

function SessionRow({ session, alert }: { session: Session; alert: boolean }) {
  const { t } = useTranslation();
  const ago = useAgo();
  return (
    <li className={styles.row}>
      <span className={styles.rowIcon}>
        <AgentIcon agent={session.agent} size={16} />
      </span>
      <div className={styles.meta}>
        <span className={styles.rowTop}>
          <span className={styles.project}>{session.project || t(`sessions.agent.${session.agent}`)}</span>
          <span className={styles.ago}>{ago(session.statusSince)}</span>
        </span>
        <span className={styles.detail}>{session.detail ?? t(`sessions.agent.${session.agent}`)}</span>
      </div>
      {alert ? (
        <TextButton variant="filled" tone={STATUS_COLOR.waiting} onClick={() => sessionActions.acknowledge(session)}>
          {t("sessions.acknowledge")}
        </TextButton>
      ) : (
        <StatusChip status={session.status} />
      )}
      <IconButton label={t("sessions.remove")} size={28} onClick={() => sessionActions.dismiss(session.id)}>
        <X size={14} />
      </IconButton>
    </li>
  );
}

export function SessionsExpanded() {
  const { sessions, acknowledged } = useSessions();
  if (!sessions.length) return <Empty />;
  return (
    <ul className={styles.list}>
      {sessions.map((s) => (
        <SessionRow key={s.id} session={s} alert={needsYou(s, acknowledged)} />
      ))}
    </ul>
  );
}

const WORKING: Activity = { priority: 50 };
const DONE: Activity = { priority: 35 };

export const sessionsApp: DuriApp = {
  id: "sessions",
  nameKey: "apps.sessions.name",
  descriptionKey: "apps.sessions.description",
  Icon: Bot,
  accent: ACCENT,
  start: startSessions,
  useActivity() {
    const { sessions, acknowledged } = useSessions();
    const alertKey = sessions
      .filter((s) => needsYou(s, acknowledged))
      .map((s) => `${s.id}:${s.statusSince}`)
      .join(",");
    const busy = sessions.some((s) => s.status === "working" || s.status === "waiting");
    const lastDone = Math.max(0, ...sessions.filter((s) => s.status === "done").map((s) => s.statusSince));
    const now = useNow(lastDone > 0, 5_000);
    const recentlyDone = lastDone > 0 && now - lastDone < DONE_VISIBLE_MS;
    return useMemo<Activity | null>(
      () => (alertKey ? { priority: 90, alert: true, key: alertKey } : busy ? WORKING : recentlyDone ? DONE : null),
      [alertKey, busy, recentlyDone],
    );
  },
  Pill: SessionsPill,
  pillWidth: 150,
  Minimal: SessionsMinimal,
  Compact: SessionsCompact,
  compactSize: { w: 392, h: 72 },
  Expanded: SessionsExpanded,
  expandedSize: { w: 460, h: 276 },
};
