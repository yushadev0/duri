import { Flag, Pause, Play, RotateCcw, Timer as TimerIcon } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { IconButton } from "../../components/Button";
import { formatStopwatch } from "../../lib/format";
import { useNow } from "../../lib/useNow";
import type { Activity, DuriApp } from "../types";
import { startTimers, stopwatchActions, stopwatchElapsed, useTimersSnapshot } from "./store";
import styles from "./timers.module.css";

const GO = "var(--accent-go)";
const STOP = "var(--accent-stop)";
const GREY = "var(--island-fg)";

function useStopwatch(precise: boolean) {
  const { stopwatch } = useTimersSnapshot();
  const running = stopwatch.runningSince !== null;
  const now = useNow(running, precise ? undefined : 250);
  return { stopwatch, running, elapsed: stopwatchElapsed(stopwatch, now) };
}

function useDecimal() {
  return useTranslation().t("format.decimal");
}

function Controls({ size }: { size: number }) {
  const { t } = useTranslation();
  const { running, elapsed } = useStopwatch(false);
  const icon = Math.round(size * 0.42);
  return (
    <>
      {running ? (
        <IconButton label={t("stopwatch.lap")} size={size} variant="tinted" tone={GREY} onClick={stopwatchActions.lap}>
          <Flag size={icon} />
        </IconButton>
      ) : (
        <IconButton
          label={t("stopwatch.reset")}
          size={size}
          variant="tinted"
          tone={GREY}
          disabled={elapsed === 0}
          onClick={stopwatchActions.reset}
        >
          <RotateCcw size={icon} />
        </IconButton>
      )}
      <IconButton
        label={running ? t("stopwatch.stop") : t("stopwatch.start")}
        size={size}
        variant="tinted"
        tone={running ? STOP : GO}
        onClick={running ? stopwatchActions.pause : stopwatchActions.start}
      >
        {running ? <Pause size={icon} fill="currentColor" /> : <Play size={icon} fill="currentColor" />}
      </IconButton>
    </>
  );
}

export function StopwatchPill() {
  const { running, elapsed } = useStopwatch(false);
  return (
    <div className={styles.pill}>
      <TimerIcon size={18} color={running ? GO : "var(--island-fg-2)"} />
      <span className={`${styles.pillTime} tabular`}>{formatStopwatch(elapsed, false)}</span>
    </div>
  );
}

export function StopwatchMinimal() {
  const { running } = useStopwatch(false);
  return <TimerIcon size={18} color={running ? GO : "var(--island-fg-2)"} />;
}

export function StopwatchCompact() {
  const { t } = useTranslation();
  const decimal = useDecimal();
  const { elapsed } = useStopwatch(true);
  return (
    <div className={styles.compact}>
      <div className={styles.meta} style={{ paddingLeft: 10 }}>
        <span className={styles.sub}>{t("stopwatch.title")}</span>
        <span className={`${styles.bigTime} tabular`}>{formatStopwatch(elapsed, true, decimal)}</span>
      </div>
      <Controls size={36} />
    </div>
  );
}

export function StopwatchExpanded() {
  const { t } = useTranslation();
  const decimal = useDecimal();
  const { stopwatch, elapsed } = useStopwatch(true);
  const { laps } = stopwatch;

  const splits = laps.map((total, i) => total - (i ? laps[i - 1] : 0));
  const current = elapsed - (laps.length ? laps[laps.length - 1] : 0);
  const ranked = splits.length >= 2;
  const fastest = ranked ? Math.min(...splits) : -1;
  const slowest = ranked ? Math.max(...splits) : -1;

  const rows = splits.map((ms, i) => ({ n: i + 1, ms, live: false }));
  if (elapsed > 0) rows.push({ n: laps.length + 1, ms: current, live: true });
  rows.reverse();

  return (
    <div className={styles.expanded}>
      <div className={styles.swHead}>
        <span className={`${styles.swTime} tabular`}>{formatStopwatch(elapsed, true, decimal)}</span>
        <div className={styles.swControls}>
          <Controls size={44} />
        </div>
      </div>
      <ul className={styles.list}>
        {rows.map(({ n, ms, live }) => {
          const tag = !live && ms === fastest ? "fastest" : !live && ms === slowest ? "slowest" : null;
          return (
            <li key={n} className={styles.lap} data-tag={tag ?? undefined}>
              <span>{t("stopwatch.lapN", { n })}</span>
              {tag && <span className={styles.lapTag}>{t(`stopwatch.${tag}`)}</span>}
              <span className="tabular">{formatStopwatch(ms, true, decimal)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

const RUNNING: Activity = { priority: 30 };
const PAUSED: Activity = { priority: 10 };

export const stopwatchApp: DuriApp = {
  id: "stopwatch",
  nameKey: "apps.stopwatch.name",
  descriptionKey: "apps.stopwatch.description",
  Icon: TimerIcon,
  accent: GO,
  start: startTimers,
  useActivity() {
    const { stopwatch } = useTimersSnapshot();
    const running = stopwatch.runningSince !== null;
    const used = running || stopwatch.accumulatedMs > 0;
    return useMemo(() => (running ? RUNNING : used ? PAUSED : null), [running, used]);
  },
  Pill: StopwatchPill,
  pillWidth: 168,
  pillSide: 70,
  Minimal: StopwatchMinimal,
  Compact: StopwatchCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: StopwatchExpanded,
  expandedSize: { w: 440, h: 276 },
};
