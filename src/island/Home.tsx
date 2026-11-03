import { Settings2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DuriApp } from "../apps/types";
import { IconButton } from "../components/Button";
import { formatClock, useHour12 } from "../lib/clock";
import { useNow } from "../lib/useNow";
import styles from "./Home.module.css";

interface HomeProps {
  apps: DuriApp[];
  onOpen: (appId: string) => void;
  onSettings: () => void;
}

/** Compact view when no app has an activity: the time and an app launcher. */
export function Home({ apps, onOpen, onSettings }: HomeProps) {
  const { t, i18n } = useTranslation();
  const hour12 = useHour12();
  const now = new Date(useNow(true, 1000));
  const time = formatClock(now, i18n.language, hour12);
  const date = now.toLocaleDateString(i18n.language, { weekday: "long", day: "numeric", month: "long" });

  return (
    <div className={styles.home}>
      <div className={styles.clock}>
        <span className={`${styles.time} tabular`}>{time}</span>
        <span className={styles.date}>{date}</span>
      </div>
      <nav className={styles.launcher} aria-label={t("island.home")}>
        {apps.filter((a) => !a.hidden).map((app) => (
          <IconButton
            key={app.id}
            label={t("island.switchTo", { name: t(app.nameKey) })}
            size={36}
            variant="tinted"
            tone={app.accent}
            onClick={() => onOpen(app.id)}
          >
            <app.Icon size={18} />
          </IconButton>
        ))}
        <IconButton label={t("island.openSettings")} size={36} variant="plain" onClick={onSettings}>
          <Settings2 size={18} />
        </IconButton>
      </nav>
    </div>
  );
}
