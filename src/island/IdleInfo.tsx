import { Cpu, MemoryStick } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { IdleInfo as IdleInfoMode } from "../lib/settings";
import { useSystemStats } from "../lib/systemStats";
import { formatClock, formatShortDate, useHour12 } from "../lib/clock";
import { useNow } from "../lib/useNow";
import styles from "./IdleInfo.module.css";

function Clock({ withDate }: { withDate: boolean }) {
  const { i18n } = useTranslation();
  const hour12 = useHour12();
  const now = new Date(useNow(true, 1000));
  const time = formatClock(now, i18n.language, hour12);
  const date = formatShortDate(now, i18n.language);
  return (
    <>
      <span className={`${styles.time} tabular`}>{time}</span>
      {withDate && (
        <>
          <span className={styles.sep} aria-hidden />
          <span className={styles.date}>{date}</span>
        </>
      )}
    </>
  );
}

function Stats() {
  const { t } = useTranslation();
  const { cpu, memory } = useSystemStats();
  return (
    <span className={styles.stats}>
      <span className={styles.stat} title={t("idle.cpu")}>
        <Cpu size={13} aria-label={t("idle.cpu")} />
        <span className="tabular">{cpu}%</span>
      </span>
      <span className={styles.stat} title={t("idle.memory")}>
        <MemoryStick size={13} aria-label={t("idle.memory")} />
        <span className="tabular">{memory}%</span>
      </span>
    </span>
  );
}

/** What the island shows when nothing is going on (Settings › General › Island). */
export function IdleInfo({ mode }: { mode: IdleInfoMode }) {
  if (mode === "none") return null;
  return (
    <div className={styles.idle}>
      {mode === "clock" && <Clock withDate />}
      {mode === "stats" && <Stats />}
      {mode === "both" && (
        <>
          <Clock withDate={false} />
          <span className={styles.sep} aria-hidden />
          <Stats />
        </>
      )}
    </div>
  );
}
