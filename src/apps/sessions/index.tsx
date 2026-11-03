import { Asterisk, Bot, Check, SquareTerminal, X } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { useMemo, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { IconButton, TextButton } from "../../components/Button";
import { useStep } from "../../lib/ticks";
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

/** Working dots light up one after another, a few repaints a second. */
function WorkingDots({ color }: { color: string }) {
  const reduceMotion = useReducedMotion();
  const step = useStep(!reduceMotion, 400);
  const on = reduceMotion ? -1 : step % 3;
  return (
    <span className={styles.dots} style={{ color }} aria-hidden>
      {[0, 1, 2].map((i) => (
        <i key={i} data-on={i === on} />
      ))}
    </span>
  );
}

function StatusGlyph({ status }: { status: Status }) {
  const color = STATUS_COLOR[status];
  if (status === "working") return <WorkingDots color={color} />;
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

/** Deny / Allow for a permission prompt, plus "Answer in Terminal" where there is room. */
function PermissionActions({ session, withTerminal = false }: { session: Session; withTerminal?: boolean }) {
  const { t } = useTranslation();
  const p = session.permission;
  if (!p) return null;
  return (
    <div className={styles.permission}>
      {withTerminal && (
        <IconButton label={t("sessions.permission.terminal")} size={32} variant="plain" onClick={() => sessionActions.answer(p.id, "terminal")}>
          <SquareTerminal size={16} />
        </IconButton>
      )}
      <TextButton variant="tinted" tone="var(--island-fg)" onClick={() => sessionActions.answer(p.id, "deny")}>
        {t("sessions.permission.deny")}
      </TextButton>
      <TextButton variant="filled" tone={STATUS_COLOR.waiting} onClick={() => sessionActions.answer(p.id, "allow")}>
        {t("sessions.permission.allow")}
      </TextButton>
    </div>
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
        <span className={styles.detail}>
          {session.permission
            ? t("sessions.permission.ask", { detail: session.permission.detail })
            : (session.detail ?? session.prompt ?? t(`sessions.agent.${session.agent}`))}
        </span>
      </div>
      {session.permission ? (
        <PermissionActions session={session} />
      ) : alert ? (
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

function pad(n: number) {
  return String(n).padStart(2, "0");
}

/** "02:13" or "1:02:13" since `since`, ticking every second. */
function Elapsed({ since, live }: { since: number; live: boolean }) {
  const now = useNow(live, 1000);
  const s = Math.max(0, Math.floor((now - since) / 1000));
  const h = Math.floor(s / 3600);
  const text = h ? `${h}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}` : `${pad(Math.floor(s / 60))}:${pad(s % 60)}`;
  return <span className="tabular">{text}</span>;
}

/** The person-facing name of where a session runs, e.g. "~/PROJECTS/duri". */
function shortPath(cwd: string) {
  const parts = cwd.split(/[\\/]/).filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : parts.join("/");
}

function useSelected() {
  const { sessions, acknowledged, selectedId } = useSessions();
  const selected = sessions.find((s) => s.id === selectedId) ?? focusSession(sessions, acknowledged);
  return { sessions, acknowledged, selected };
}

function StepCard({ session, alert }: { session: Session; alert: boolean }) {
  const { t } = useTranslation();
  const color = STATUS_COLOR[session.status];
  const label =
    session.permission
      ? t("sessions.permission.label")
      : session.status === "waiting"
      ? t("sessions.status.waiting")
      : session.status === "working"
        ? t("sessions.step.now")
        : t("sessions.step.last");
  const text =
    session.detail ??
    (session.status === "done" ? t("sessions.step.finished") : session.status === "idle" ? t("sessions.step.ready") : "…");
  return (
    <div className={styles.card} style={{ "--tone": color } as CSSProperties} role={alert ? "alert" : undefined}>
      <div className={styles.cardText}>
        <span className={styles.cardLabel}>{label}</span>
        <span className={styles.cardDetail}>{text}</span>
      </div>
      {session.permission ? (
        <PermissionActions session={session} withTerminal />
      ) : (
        alert && (
          <TextButton variant="filled" tone={color} onClick={() => sessionActions.acknowledge(session)}>
            {t("sessions.acknowledge")}
          </TextButton>
        )
      )}
    </div>
  );
}

function Switcher({ sessions, selected }: { sessions: Session[]; selected: Session }) {
  const { t } = useTranslation();
  return (
    <div className={styles.switcher} role="tablist" aria-label={t("apps.sessions.name")}>
      {sessions.map((s) => (
        <button
          key={s.id}
          type="button"
          role="tab"
          aria-selected={s.id === selected.id}
          className={styles.tab}
          onClick={(e) => {
            e.stopPropagation();
            sessionActions.select(s.id);
          }}
        >
          <span className={styles.tabDot} style={{ background: STATUS_COLOR[s.status] }} aria-hidden />
          <span className={styles.tabName}>{s.project || t(`sessions.agent.${s.agent}`)}</span>
        </button>
      ))}
    </div>
  );
}

export function SessionsExpanded() {
  const { t } = useTranslation();
  const { sessions, acknowledged, selected } = useSelected();
  if (!selected) return <Empty />;
  const alert = needsYou(selected, acknowledged);
  const color = STATUS_COLOR[selected.status];
  const live = selected.status === "working" || selected.status === "waiting";

  return (
    <div className={styles.expanded}>
      <div className={styles.head}>
        <span className={styles.bigTile} style={{ "--tone": color } as CSSProperties} data-status={selected.status}>
          <AgentIcon agent={selected.agent} size={30} />
        </span>
        <div className={styles.meta}>
          <span className={styles.bigProject}>{selected.project || t(`sessions.agent.${selected.agent}`)}</span>
          {selected.prompt && <span className={styles.prompt}>“{selected.prompt}”</span>}
          <span className={styles.where}>
            {t(`sessions.agent.${selected.agent}`)}
            {selected.cwd && ` · ${shortPath(selected.cwd)}`}
          </span>
        </div>
        <div className={styles.headSide}>
          <span className={styles.statusLine} style={{ color }}>
            <StatusGlyph status={selected.status} />
            {t(`sessions.status.${selected.status}`)}
          </span>
          <span className={styles.elapsed}>
            <Elapsed since={selected.statusSince} live={live} />
          </span>
        </div>
      </div>
      <StepCard session={selected} alert={alert} />
      <div className={styles.footer}>
        {sessions.length > 1 ? <Switcher sessions={sessions} selected={selected} /> : <span />}
        <IconButton label={t("sessions.remove")} size={28} onClick={() => sessionActions.dismiss(selected.id)}>
          <X size={14} />
        </IconButton>
      </div>
    </div>
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
  useBackdrop() {
    const { selected } = useSelected();
    if (!selected) return null;
    const color = STATUS_COLOR[selected.status];
    // Same glow as Music's cover gradient, tinted by status.
    return `radial-gradient(130% 150% at 0% 0%, color-mix(in srgb, ${color} 34%, black) 0%, color-mix(in srgb, ${color} 12%, black) 40%, #000 76%)`;
  },
  Pill: SessionsPill,
  pillWidth: 150,
  pillSide: 62,
  Minimal: SessionsMinimal,
  Compact: SessionsCompact,
  compactSize: { w: 392, h: 72 },
  Expanded: SessionsExpanded,
  expandedSize: { w: 460, h: 246 },
};
