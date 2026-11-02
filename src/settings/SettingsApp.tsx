import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Info, LayoutGrid, SlidersHorizontal } from "lucide-react";
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { registry } from "../apps/registry";
import { refreshHooks, sessionActions, useHookStatus } from "../apps/sessions/store";
import { DuriMark } from "../components/DuriMark";
import { updateSettings, useSettings, type IslandStyle, type LanguagePref, type PillSize } from "../lib/settings";
import styles from "./Settings.module.css";

type Pane = "general" | "apps" | "about";
const PANES: { id: Pane; Icon: typeof Info }[] = [
  { id: "general", Icon: SlidersHorizontal },
  { id: "apps", Icon: LayoutGrid },
  { id: "about", Icon: Info },
];
const PANE_KEY = "duri.settings.pane";
const REPO_URL = "https://github.com/yushadev0/duri";

function loadPane(): Pane {
  try {
    const p = localStorage.getItem(PANE_KEY);
    if (p === "general" || p === "apps" || p === "about") return p;
  } catch {
    // storage unavailable
  }
  return "general";
}

function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={styles.switch}
      onClick={() => onChange(!checked)}
    >
      <span className={styles.knob} />
    </button>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className={styles.segmented} role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          className={styles.segment}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Row({ title, hint, children, icon }: { title: string; hint?: string; children?: ReactNode; icon?: ReactNode }) {
  return (
    <div className={styles.row}>
      {icon}
      <div className={styles.rowText}>
        <span className={styles.rowTitle}>{title}</span>
        {hint && <span className={styles.rowHint}>{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function General() {
  const { t } = useTranslation();
  const settings = useSettings();
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    isEnabled().then(setAutostart).catch(() => setAutostart(false));
  }, []);

  const toggleAutostart = async (on: boolean) => {
    setAutostart(on);
    try {
      await (on ? enable() : disable());
    } catch {
      setAutostart(!on);
    }
  };

  return (
    <>
      <section className={styles.group}>
        <Row title={t("settings.language")}>
          <select
            className={styles.select}
            value={settings.language}
            aria-label={t("settings.language")}
            onChange={(e) => updateSettings({ language: e.target.value as LanguagePref })}
          >
            <option value="system">{t("settings.languageSystem")}</option>
            <option value="en">English</option>
            <option value="tr">Türkçe</option>
          </select>
        </Row>
        <Row title={t("settings.launchAtLogin")} hint={t("settings.launchAtLoginHint")}>
          <Switch checked={!!autostart} onChange={toggleAutostart} label={t("settings.launchAtLogin")} />
        </Row>
      </section>
      <h2 className={styles.groupTitle}>{t("settings.island")}</h2>
      <section className={styles.group}>
        <Row title={t("settings.islandStyle")} hint={t("settings.islandStyleHint")}>
          <Segmented<IslandStyle>
            label={t("settings.islandStyle")}
            value={settings.islandStyle}
            onChange={(islandStyle) => updateSettings({ islandStyle })}
            options={[
              { value: "floating", label: t("settings.floating") },
              { value: "attached", label: t("settings.attached") },
            ]}
          />
        </Row>
        <Row title={t("settings.pillSize")}>
          <Segmented<PillSize>
            label={t("settings.pillSize")}
            value={settings.pillSize}
            onChange={(pillSize) => updateSettings({ pillSize })}
            options={[
              { value: "compact", label: t("settings.sizeCompact") },
              { value: "default", label: t("settings.sizeDefault") },
              { value: "large", label: t("settings.sizeLarge") },
            ]}
          />
        </Row>
        <Row title={t("settings.replaceFlyout")} hint={t("settings.replaceFlyoutHint")}>
          <Switch
            checked={settings.replaceVolumeFlyout}
            onChange={(replaceVolumeFlyout) => updateSettings({ replaceVolumeFlyout })}
            label={t("settings.replaceFlyout")}
          />
        </Row>
        <Row title={t("settings.usage")} hint={t("settings.usageHint")} />
      </section>
    </>
  );
}

function Apps() {
  const { t } = useTranslation();
  const settings = useSettings();
  const setEnabled = (id: string, on: boolean) =>
    updateSettings({
      disabledApps: on ? settings.disabledApps.filter((x) => x !== id) : [...settings.disabledApps, id],
    });

  return (
    <>
      <p className={styles.paneHint}>{t("settings.appsHint")}</p>
      <section className={styles.group}>
        {registry.map((app) => (
          <Row
            key={app.id}
            title={t(app.nameKey)}
            hint={t(app.descriptionKey)}
            icon={
              <span className={styles.appIcon} style={{ "--tone": app.accent } as CSSProperties}>
                <app.Icon size={18} />
              </span>
            }
          >
            <Switch
              checked={!settings.disabledApps.includes(app.id)}
              onChange={(on) => setEnabled(app.id, on)}
              label={t(app.nameKey)}
            />
          </Row>
        ))}
      </section>
      <Integrations />
    </>
  );
}

function Integrations() {
  const { t } = useTranslation();
  const hooks = useHookStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refreshHooks();
  }, []);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const hint = !hooks
    ? undefined
    : !hooks.serverRunning
      ? t("settings.serverDown")
      : hooks.installed
        ? t("settings.claudeConnected")
        : t("settings.claudeNotConnected");

  return (
    <>
      <h2 className={styles.groupTitle}>{t("settings.integrations")}</h2>
      <section className={styles.group}>
        <Row title="Claude Code" hint={error ?? hint}>
          {hooks && (
            <button
              type="button"
              className={hooks.installed ? styles.button : styles.buttonPrimary}
              disabled={busy}
              onClick={() => run(hooks.installed ? sessionActions.uninstallHooks : sessionActions.installHooks)}
            >
              {hooks.installed ? t("settings.disconnect") : t("settings.connect")}
            </button>
          )}
        </Row>
        <Row title="Codex" hint={t("settings.codexHint")} />
      </section>
    </>
  );
}

function About() {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);
  return (
    <div className={styles.about}>
      <DuriMark size={88} />
      <h1 className={styles.aboutName}>Duri</h1>
      <p className={styles.aboutTagline}>{t("settings.tagline")}</p>
      {version && <p className={styles.aboutMeta}>{t("settings.version", { version })}</p>}
      <button type="button" className={styles.linkButton} onClick={() => openUrl(REPO_URL)}>
        {t("settings.github")}
      </button>
      <p className={styles.aboutMeta}>{t("settings.license")}</p>
    </div>
  );
}

export function SettingsApp() {
  const { t } = useTranslation();
  const [pane, setPane] = useState<Pane>(loadPane);

  useEffect(() => {
    try {
      localStorage.setItem(PANE_KEY, pane);
    } catch {
      // ignore
    }
    getCurrentWindow()
      .setTitle(`${t(`settings.panes.${pane}`)} — Duri`)
      .catch(() => {});
  }, [pane, t]);

  return (
    <div className={styles.window}>
      <nav className={styles.toolbar} role="tablist" aria-label={t("settings.windowTitle")}>
        {PANES.map(({ id, Icon }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={pane === id}
            className={styles.tab}
            onClick={() => setPane(id)}
          >
            <Icon size={20} />
            <span>{t(`settings.panes.${id}`)}</span>
          </button>
        ))}
      </nav>
      <main className={styles.content} role="tabpanel">
        {pane === "general" && <General />}
        {pane === "apps" && <Apps />}
        {pane === "about" && <About />}
      </main>
    </div>
  );
}
