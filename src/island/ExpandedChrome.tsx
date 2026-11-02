import { Settings2 } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { DuriApp } from "../apps/types";
import { IconButton } from "../components/Button";
import styles from "./ExpandedChrome.module.css";
import { CONTENT_PADDING, EXPANDED_DOCK } from "./geometry";

interface Props {
  apps: DuriApp[];
  current: DuriApp;
  onSelect: (appId: string) => void;
  onSettings: () => void;
  children: ReactNode;
}

/**
 * Frame of every expanded app: the app's content on top, a dock of all apps
 * at the bottom. The current app is shown with its name in its own color.
 */
export function ExpandedChrome({ apps, current, onSelect, onSettings, children }: Props) {
  const { t } = useTranslation();
  return (
    <div
      className={styles.expanded}
      style={{ "--dock": `${EXPANDED_DOCK}px`, "--pad": `${CONTENT_PADDING}px` } as CSSProperties}
    >
      <div className={styles.body}>{children}</div>
      <nav className={styles.dock} aria-label={t("island.home")}>
        <div className={styles.apps}>
          {apps.filter((a) => !a.hidden || a.id === current.id).map((app) =>
            app.id === current.id ? (
              <span
                key={app.id}
                className={styles.current}
                style={{ "--tone": app.accent } as CSSProperties}
                aria-current="page"
              >
                <app.Icon size={15} />
                {t(app.nameKey)}
              </span>
            ) : (
              <IconButton
                key={app.id}
                label={t("island.switchTo", { name: t(app.nameKey) })}
                size={30}
                onClick={() => onSelect(app.id)}
              >
                <app.Icon size={15} />
              </IconButton>
            ),
          )}
        </div>
        <IconButton label={t("island.openSettings")} size={30} onClick={onSettings}>
          <Settings2 size={15} />
        </IconButton>
      </nav>
    </div>
  );
}
