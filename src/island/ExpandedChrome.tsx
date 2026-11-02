import { Settings2 } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { DuriApp } from "../apps/types";
import { IconButton } from "../components/Button";
import styles from "./ExpandedChrome.module.css";
import { CONTENT_PADDING, EXPANDED_HEADER } from "./geometry";

interface Props {
  apps: DuriApp[];
  current: DuriApp;
  onSelect: (appId: string) => void;
  onSettings: () => void;
  children: ReactNode;
}

/** Shared header of every expanded app: its name, then the other apps. */
export function ExpandedChrome({ apps, current, onSelect, onSettings, children }: Props) {
  const { t } = useTranslation();
  return (
    <div
      className={styles.expanded}
      style={{ "--header": `${EXPANDED_HEADER}px`, "--pad": `${CONTENT_PADDING}px` } as CSSProperties}
    >
      <header className={styles.header}>
        <span className={styles.title} style={{ color: current.accent }}>
          <current.Icon size={15} />
          {t(current.nameKey)}
        </span>
        <nav className={styles.switcher}>
          {apps
            .filter((a) => a.id !== current.id)
            .map((app) => (
              <IconButton
                key={app.id}
                label={t("island.switchTo", { name: t(app.nameKey) })}
                size={28}
                onClick={() => onSelect(app.id)}
              >
                <app.Icon size={15} />
              </IconButton>
            ))}
          <IconButton label={t("island.openSettings")} size={28} onClick={onSettings}>
            <Settings2 size={15} />
          </IconButton>
        </nav>
      </header>
      <div className={styles.body}>{children}</div>
    </div>
  );
}
