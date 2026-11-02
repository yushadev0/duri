import { Hourglass, Minus, Pause, Play, Plus, RotateCcw, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconButton, TextButton } from "../../components/Button";
import { Ring } from "../../components/Ring";
import { formatCountdown } from "../../lib/format";
import { useNow } from "../../lib/useNow";
import type { Activity, DuriApp } from "../types";
import {
  durationLabel,
  primaryTimer,
  remainingMs,
  startTimers,
  timerActions,
  useTimersSnapshot,
  type Timer,
} from "./store";
import styles from "./timers.module.css";

const ORANGE = "var(--accent-timer)";
const GREY = "var(--island-fg)";
const MIN = 60_000;
const PRESETS = [1, 5, 10, 25].map((m) => m * MIN);
const STEPS = [1, 2, 3, 4, 5, 10, 15, 20, 25, 30, 45, 60, 90, 120, 180].map((m) => m * MIN);

function useTimerNow(timers: Timer[]) {
  return useNow(timers.some((t) => t.state === "running"), 200);
}

function TimerRing({ timer, now, size, stroke }: { timer: Timer; now: number; size: number; stroke?: number }) {
  return <Ring progress={remainingMs(timer, now) / timer.durationMs} size={size} stroke={stroke} color={ORANGE} />;
}

function PauseResume({ timer, size = 36 }: { timer: Timer; size?: number }) {
  const { t } = useTranslation();
  const running = timer.state === "running";
  return (
    <IconButton
      label={running ? t("timer.pause") : t("timer.resume")}
      size={size}
      variant="tinted"
      tone={ORANGE}
      onClick={() => (running ? timerActions.pause(timer.id) : timerActions.resume(timer.id))}
    >
      {running ? <Pause size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" />}
    </IconButton>
  );
}

export function TimerPill() {
  const { timers } = useTimersSnapshot();
  const now = useTimerNow(timers);
  const timer = primaryTimer(timers);
  if (!timer) return null;
  return (
    <div className={styles.pill}>
      <TimerRing timer={timer} now={now} size={20} />
      <span className={`${styles.pillTime} tabular`} style={{ color: ORANGE }}>
        {formatCountdown(remainingMs(timer, now))}
      </span>
    </div>
  );
}

export function TimerMinimal() {
  const { timers } = useTimersSnapshot();
  const now = useTimerNow(timers);
  const timer = primaryTimer(timers);
  return timer ? <TimerRing timer={timer} now={now} size={22} /> : null;
}

export function TimerCompact() {
  const { t } = useTranslation();
  const { timers } = useTimersSnapshot();
  const now = useTimerNow(timers);
  const timer = primaryTimer(timers);
  if (!timer) return null;

  if (timer.state === "finished") {
    return (
      <div className={styles.compact} role="alert">
        <span className={styles.doneIcon} style={{ color: ORANGE }}>
          <Hourglass size={22} />
        </span>
        <div className={styles.meta}>
          <span className={styles.bigLabel}>{t("timer.done")}</span>
          <span className={styles.sub}>{timer.label ?? durationLabel(timer.durationMs)}</span>
        </div>
        <TextButton variant="tinted" tone={GREY} onClick={() => timerActions.restart(timer.id)}>
          {t("timer.repeat")}
        </TextButton>
        <TextButton variant="filled" tone={ORANGE} onClick={() => timerActions.remove(timer.id)}>
          {t("timer.dismiss")}
        </TextButton>
      </div>
    );
  }

  return (
    <div className={styles.compact}>
      <TimerRing timer={timer} now={now} size={44} stroke={4} />
      <div className={styles.meta}>
        <span className={styles.sub}>{timer.label ?? durationLabel(timer.durationMs)}</span>
        <span className={`${styles.bigTime} tabular`} style={{ color: ORANGE }}>
          {formatCountdown(remainingMs(timer, now))}
        </span>
      </div>
      <PauseResume timer={timer} />
      <IconButton label={t("timer.cancel")} size={36} variant="tinted" tone={GREY} onClick={() => timerActions.remove(timer.id)}>
        <X size={16} />
      </IconButton>
    </div>
  );
}

function TimerRow({ timer, now }: { timer: Timer; now: number }) {
  const { t } = useTranslation();
  const done = timer.state === "finished";
  return (
    <li className={styles.row}>
      <TimerRing timer={timer} now={now} size={28} />
      <div className={styles.meta}>
        <span className={`${styles.rowTime} tabular`} style={{ color: done ? ORANGE : undefined }}>
          {done ? t("timer.done") : formatCountdown(remainingMs(timer, now))}
        </span>
        <span className={styles.sub}>{timer.label ?? durationLabel(timer.durationMs)}</span>
      </div>
      {done ? (
        <IconButton label={t("timer.repeat")} size={30} variant="tinted" tone={ORANGE} onClick={() => timerActions.restart(timer.id)}>
          <RotateCcw size={14} />
        </IconButton>
      ) : (
        <PauseResume timer={timer} size={30} />
      )}
      <IconButton
        label={done ? t("timer.dismiss") : t("timer.cancel")}
        size={30}
        variant="tinted"
        tone={GREY}
        onClick={() => timerActions.remove(timer.id)}
      >
        <X size={14} />
      </IconButton>
    </li>
  );
}

export function TimerExpanded() {
  const { t } = useTranslation();
  const { timers } = useTimersSnapshot();
  const now = useTimerNow(timers);
  const [step, setStep] = useState(6); // 15 min
  const custom = STEPS[step];

  return (
    <div className={styles.expanded}>
      {timers.length ? (
        <ul className={styles.list}>
          {timers.map((timer) => (
            <TimerRow key={timer.id} timer={timer} now={now} />
          ))}
        </ul>
      ) : (
        <p className={styles.emptyList}>{t("timer.noTimers")}</p>
      )}
      <div className={styles.presets}>
        {PRESETS.map((ms) => (
          <TextButton key={ms} variant="tinted" tone={ORANGE} onClick={() => timerActions.start(ms)}>
            {durationLabel(ms)}
          </TextButton>
        ))}
      </div>
      <div className={styles.custom}>
        <IconButton label={t("timer.decrease")} size={32} variant="tinted" tone={GREY} disabled={step === 0} onClick={() => setStep(step - 1)}>
          <Minus size={16} />
        </IconButton>
        <span className={`${styles.customValue} tabular`} aria-live="polite">
          {durationLabel(custom)}
        </span>
        <IconButton
          label={t("timer.increase")}
          size={32}
          variant="tinted"
          tone={GREY}
          disabled={step === STEPS.length - 1}
          onClick={() => setStep(step + 1)}
        >
          <Plus size={16} />
        </IconButton>
        <span className={styles.spacer} />
        <TextButton variant="filled" tone={ORANGE} onClick={() => timerActions.start(custom)}>
          {t("timer.start")}
        </TextButton>
      </div>
    </div>
  );
}

const RUNNING: Activity = { priority: 60 };

export const timerApp: DuriApp = {
  id: "timer",
  nameKey: "apps.timer.name",
  descriptionKey: "apps.timer.description",
  Icon: Hourglass,
  accent: ORANGE,
  start: startTimers,
  useActivity() {
    const { timers } = useTimersSnapshot();
    const finishedKey = timers
      .filter((t) => t.state === "finished")
      .map((t) => t.id)
      .join(",");
    const any = timers.length > 0;
    return useMemo<Activity | null>(
      () => (finishedKey ? { priority: 100, alert: true, key: finishedKey } : any ? RUNNING : null),
      [finishedKey, any],
    );
  },
  Pill: TimerPill,
  pillWidth: 168,
  pillSide: 66,
  Minimal: TimerMinimal,
  Compact: TimerCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: TimerExpanded,
  expandedSize: { w: 440, h: 276 },
};
