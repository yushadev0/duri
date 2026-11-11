import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Info, LayoutGrid, SlidersHorizontal } from "lucide-react";
import { Fragment, useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { registry } from "../apps/registry";
import { refreshHooks, sessionActions, useHookStatus } from "../apps/sessions/store";
import { DuriMark } from "../components/DuriMark";
import { updateSettings, useSettings, type ClockFormat, type IdleInfo, type IslandStyle, type LanguagePref, type PillSize, type Transparency } from "../lib/settings";
import { checkForUpdates, installUpdate, useUpdateStatus } from "../lib/updates";
import { acceleratorFromEvent, acceleratorKeys } from "../lib/shortcuts";
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

/**
 * Click, then press the shortcut. Esc cancels, Backspace turns it off.
 */
function ShortcutField({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const keys = acceleratorKeys(value);
  return (
    <button
      type="button"
      className={styles.shortcut}
      data-recording={recording}
      aria-label={`${label}: ${keys.length ? keys.join(" ") : t("settings.shortcutOff")}`}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        if (e.key === "Escape") return setRecording(false);
        if ((e.key === "Backspace" || e.key === "Delete") && !e.ctrlKey && !e.altKey && !e.metaKey) {
          onChange("");
          return setRecording(false);
        }
        const accel = acceleratorFromEvent(e);
        if (accel) {
          onChange(accel);
          setRecording(false);
        }
      }}
    >
      {recording ? (
        <span className={styles.shortcutHint}>{t("settings.shortcutRecord")}</span>
      ) : keys.length ? (
        keys.map((k, i) => (
          <kbd key={i} className={styles.key}>
            {k}
          </kbd>
        ))
      ) : (
        <span className={styles.shortcutHint}>{t("settings.shortcutOff")}</span>
      )}
    </button>
  );
}

function Shortcuts() {
  const { t } = useTranslation();
  const settings = useSettings();
  const [failed, setFailed] = useState<string[]>([]);
  useEffect(() => {
    invoke<string[]>("shortcuts_failed").then(setFailed).catch(() => {});
    const un = listen<string[]>("shortcuts://failed", (e) => setFailed(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);
  const rows = [
    { action: "island", key: "shortcutIsland", title: t("settings.shortcutIsland") },
    { action: "note", key: "shortcutNote", title: t("settings.shortcutNote") },
    { action: "clipboard", key: "shortcutClipboard", title: t("settings.shortcutClipboard") },
  ] as const;
  return (
    <>
      <h2 className={styles.groupTitle}>{t("settings.shortcuts")}</h2>
      <section className={styles.group}>
        {rows.map((r) => (
          <Row key={r.action} title={r.title} hint={failed.includes(r.action) ? t("settings.shortcutTaken") : undefined}>
            <ShortcutField
              value={settings[r.key]}
              label={r.title}
              onChange={(v) => updateSettings({ [r.key]: v })}
            />
          </Row>
        ))}
      </section>
      <p className={styles.paneHint}>{t("settings.shortcutsHint")}</p>
    </>
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
        <Row title={t("updates.auto")} hint={t("updates.autoHint")}>
          <Switch
            checked={settings.autoUpdateCheck}
            onChange={(autoUpdateCheck) => updateSettings({ autoUpdateCheck })}
            label={t("updates.auto")}
          />
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
        <Row title={t("settings.transparency")} hint={t("settings.transparencyHint")}>
          <Segmented<Transparency>
            label={t("settings.transparency")}
            value={settings.transparency}
            onChange={(transparency) => updateSettings({ transparency })}
            options={[
              { value: "off", label: t("settings.transparencyOff") },
              { value: "low", label: t("settings.transparencyLow") },
              { value: "high", label: t("settings.transparencyHigh") },
            ]}
          />
        </Row>
        <Row title={t("settings.idleInfo")} hint={t("settings.idleInfoHint")}>
          <Segmented<IdleInfo>
            label={t("settings.idleInfo")}
            value={settings.idleInfo}
            onChange={(idleInfo) => updateSettings({ idleInfo })}
            options={[
              { value: "none", label: t("settings.idleNone") },
              { value: "clock", label: t("settings.idleClock") },
              { value: "stats", label: t("settings.idleStats") },
              { value: "both", label: t("settings.idleBoth") },
            ]}
          />
        </Row>
        <Row title={t("settings.clockFormat")} hint={t("settings.clockFormatHint")}>
          <Segmented<ClockFormat>
            label={t("settings.clockFormat")}
            value={settings.clockFormat}
            onChange={(clockFormat) => updateSettings({ clockFormat })}
            options={[
              { value: "system", label: t("settings.clockSystem") },
              { value: "12h", label: t("settings.clock12") },
              { value: "24h", label: t("settings.clock24") },
            ]}
          />
        </Row>
        <Row title={t("settings.autoHide")} hint={t("settings.autoHideHint")}>
          <Switch
            checked={settings.autoHide}
            onChange={(autoHide) => updateSettings({ autoHide })}
            label={t("settings.autoHide")}
          />
        </Row>
        <Row title={t("settings.hideInFullscreen")} hint={t("settings.hideInFullscreenHint")}>
          <Switch
            checked={settings.hideInFullscreen}
            onChange={(hideInFullscreen) => updateSettings({ hideInFullscreen })}
            label={t("settings.hideInFullscreen")}
          />
        </Row>
        <Row title={t("settings.usage")} hint={t("settings.usageHint")} />
      </section>
      <Shortcuts />
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
        {registry.map((app) => {
          const on = !settings.disabledApps.includes(app.id);
          return (
            <Fragment key={app.id}>
              <Row
                title={t(app.nameKey)}
                hint={t(app.descriptionKey)}
                icon={
                  <span className={styles.appIcon} style={{ "--tone": app.accent } as CSSProperties}>
                    <app.Icon size={18} />
                  </span>
                }
              >
                <Switch checked={on} onChange={(v) => setEnabled(app.id, v)} label={t(app.nameKey)} />
              </Row>
              {app.id === "volume" && on && (
                <Row
                  title={t("settings.replaceFlyout")}
                  hint={t("settings.replaceFlyoutHint")}
                  icon={<span className={styles.appIconSpacer} />}
                >
                  <Switch
                    checked={settings.replaceVolumeFlyout}
                    onChange={(replaceVolumeFlyout) => updateSettings({ replaceVolumeFlyout })}
                    label={t("settings.replaceFlyout")}
                  />
                </Row>
              )}
              {app.id === "privacy" && on && (
                <>
                  <Row title={t("settings.privacyCamera")} icon={<span className={styles.appIconSpacer} />}>
                    <Switch
                      checked={settings.privacyCamera}
                      onChange={(privacyCamera) => updateSettings({ privacyCamera })}
                      label={t("settings.privacyCamera")}
                    />
                  </Row>
                  <Row title={t("settings.privacyMicrophone")} icon={<span className={styles.appIconSpacer} />}>
                    <Switch
                      checked={settings.privacyMicrophone}
                      onChange={(privacyMicrophone) => updateSettings({ privacyMicrophone })}
                      label={t("settings.privacyMicrophone")}
                    />
                  </Row>
                </>
              )}
            </Fragment>
          );
        })}
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
        {hooks?.installed && (
          <Row title={t("settings.claudePermissions")} hint={t("settings.claudePermissionsHint")}>
            <Switch
              checked={hooks.permissions}
              onChange={(on) => run(() => sessionActions.setPermissions(on))}
              label={t("settings.claudePermissions")}
            />
          </Row>
        )}
        <Row title="Codex" hint={t("settings.codexHint")} />
      </section>
    </>
  );
}

function Updates() {
  const { t } = useTranslation();
  const status = useUpdateStatus();

  useEffect(() => {
    if (status.kind === "idle" && !import.meta.env.DEV) checkForUpdates();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  if (status.kind === "available" || status.kind === "downloading") {
    const downloading = status.kind === "downloading";
    const label = downloading
      ? status.progress === null
        ? t("updates.downloading")
        : t("updates.downloadingProgress", { percent: Math.round(status.progress * 100) })
      : t("updates.install", { version: status.version });
    return (
      <>
        <button type="button" className={styles.linkButton} disabled={downloading} onClick={installUpdate}>
          {label}
        </button>
        <p className={styles.aboutMeta}>{t("updates.restartHint")}</p>
      </>
    );
  }

  const note =
    status.kind === "checking"
      ? t("updates.checking")
      : status.kind === "upToDate"
        ? t("updates.upToDate")
        : status.kind === "error"
          ? t("updates.failed")
          : null;
  return (
    <>
      <button
        type="button"
        className={styles.button}
        disabled={status.kind === "checking"}
        onClick={() => checkForUpdates()}
      >
        {t("updates.check")}
      </button>
      <p className={styles.aboutMeta} role="status" title={status.kind === "error" ? status.message : undefined}>
        {note ?? " "}
      </p>
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
      <div className={styles.updates}>
        <Updates />
      </div>
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
