import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Fragment, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { updateSettings, useSettings, type ClockAlign, type Settings } from "../lib/settings";
import { Row, Segmented, Switch } from "./controls";
import styles from "./Settings.module.css";

type ClockStatus = "off" | "starting" | "active" | "noTaskbar" | "noClock" | "failed" | "blocked";
const PROBLEMS: ClockStatus[] = ["noTaskbar", "noClock", "failed", "blocked"];

const LINE_KEYS = ["taskbarClockLine1", "taskbarClockLine2", "taskbarClockLine3"] as const satisfies readonly (keyof Settings)[];

/** One token of the template language with what it shows right now. */
interface ReferenceRow {
  id: string;
  group: string;
  template: string;
  example: string;
}

interface Place {
  name: string;
  region: string;
  latitude: number;
  longitude: number;
}

/** One template line. Keeps its own draft so a settings echo never moves the caret. */
function ClockLine({
  value,
  onChange,
  onFocus,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  onFocus: () => void;
  label: string;
}) {
  const [draft, setDraft] = useState(value);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (document.activeElement !== input.current) setDraft(value);
  }, [value]);
  return (
    <input
      ref={input}
      type="text"
      className={styles.input}
      value={draft}
      aria-label={label}
      spellCheck={false}
      maxLength={120}
      onFocus={onFocus}
      onChange={(e) => {
        setDraft(e.target.value);
        onChange(e.target.value);
      }}
    />
  );
}

/** Picks the city the weather tokens are for. */
function CityPicker() {
  const { t, i18n } = useTranslation();
  const settings = useSettings();
  const [query, setQuery] = useState("");
  const [found, setPlaces] = useState<Place[] | null>(null);
  const searching = query.trim().length >= 2;
  const places = searching ? found : null;

  useEffect(() => {
    if (!searching) return;
    let stale = false;
    const timer = setTimeout(() => {
      invoke<Place[]>("taskbar_clock_search_city", { query, language: i18n.language })
        .then((found) => !stale && setPlaces(found))
        .catch(() => !stale && setPlaces([]));
    }, 350);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [query, searching, i18n.language]);

  const choose = (place: Place) => {
    setQuery("");
    setPlaces(null);
    updateSettings({
      taskbarClockCity: place.name,
      taskbarClockLatitude: place.latitude,
      taskbarClockLongitude: place.longitude,
    });
  };

  return (
    <>
      <Row
        title={t("settings.taskbarClock.city")}
        hint={settings.taskbarClockCity || t("settings.taskbarClock.cityNone")}
      >
        <input
          type="search"
          className={styles.input}
          value={query}
          placeholder={t("settings.taskbarClock.citySearch")}
          aria-label={t("settings.taskbarClock.citySearch")}
          spellCheck={false}
          onChange={(e) => setQuery(e.target.value)}
        />
      </Row>
      {places && (
        <div className={styles.places} role="listbox" aria-label={t("settings.taskbarClock.city")}>
          {places.length === 0 && <span className={styles.rowHint}>{t("settings.taskbarClock.cityNoResults")}</span>}
          {places.map((place) => (
            <button
              key={`${place.latitude},${place.longitude}`}
              type="button"
              role="option"
              aria-selected={false}
              className={styles.place}
              onClick={() => choose(place)}
            >
              <span className={styles.rowTitle}>{place.name}</span>
              <span className={styles.rowHint}>{place.region}</span>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

/**
 * Replaces the text of the Windows tray clock. The only feature that runs
 * code inside explorer.exe, so switching it on always shows what that means.
 * Everything below the switch only exists while it is on; off, the taskbar
 * is exactly as Windows draws it.
 */
export function TaskbarClockPane() {
  const { t } = useTranslation();
  const settings = useSettings();
  const [status, setStatus] = useState<ClockStatus>("off");
  const [confirming, setConfirming] = useState(false);
  const [preview, setPreview] = useState("");
  const [reference, setReference] = useState<ReferenceRow[]>([]);
  /** The line a clicked token is added to: the one last typed in. */
  const [target, setTarget] = useState(0);
  const on = settings.taskbarClock;
  const lines = LINE_KEYS.map((key) => settings[key]);
  const linesKey = lines.join("\n");

  useEffect(() => {
    invoke<ClockStatus>("taskbar_clock_status").then(setStatus).catch(() => {});
    const un = listen<ClockStatus>("taskbar-clock://status", (e) => setStatus(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!on) return;
    const refresh = () => {
      invoke<string>("taskbar_clock_preview", { lines: linesKey.split("\n") }).then(setPreview).catch(() => {});
      invoke<ReferenceRow[]>("taskbar_clock_reference").then(setReference).catch(() => {});
    };
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => clearInterval(timer);
  }, [on, linesKey]);

  const problem = PROBLEMS.includes(status) ? t(`settings.taskbarClock.${status}`) : undefined;
  const groups = [...new Set(reference.map((row) => row.group))];
  const addToken = (template: string) => {
    const key = LINE_KEYS[target];
    const current = settings[key];
    updateSettings({ [key]: current && !current.endsWith(" ") ? `${current} ${template}` : current + template });
  };

  return (
    <>
      <section className={styles.group}>
        <Row title={t("settings.taskbarClock.enable")} hint={problem ?? t("settings.taskbarClock.hint")}>
          <Switch
            checked={on}
            onChange={(next) => (next ? setConfirming(true) : updateSettings({ taskbarClock: false }))}
            label={t("settings.taskbarClock.enable")}
          />
        </Row>
        {confirming && !on && (
          <div className={styles.notice} role="alertdialog" aria-label={t("settings.taskbarClock.warningTitle")}>
            <span className={styles.rowTitle}>{t("settings.taskbarClock.warningTitle")}</span>
            <span className={styles.rowHint}>{t("settings.taskbarClock.warning")}</span>
            <div className={styles.noticeActions}>
              <button type="button" className={styles.button} onClick={() => setConfirming(false)}>
                {t("settings.taskbarClock.cancel")}
              </button>
              <button
                type="button"
                className={styles.buttonPrimary}
                onClick={() => {
                  setConfirming(false);
                  updateSettings({ taskbarClock: true });
                }}
              >
                {t("settings.taskbarClock.turnOn")}
              </button>
            </div>
          </div>
        )}
      </section>
      {!on && <p className={styles.paneNote}>{t("settings.taskbarClock.offNote")}</p>}
      {on && (
        <>
          <h2 className={styles.groupTitle}>{t("settings.taskbarClock.text")}</h2>
          <section className={styles.group}>
            {LINE_KEYS.map((key, index) => (
              <Row
                key={key}
                title={t("settings.taskbarClock.line", { number: index + 1 })}
                hint={index === 2 ? t("settings.taskbarClock.line3Hint") : undefined}
              >
                <ClockLine
                  value={settings[key]}
                  label={t("settings.taskbarClock.line", { number: index + 1 })}
                  onFocus={() => setTarget(index)}
                  onChange={(value) => updateSettings({ [key]: value })}
                />
              </Row>
            ))}
            <Row title={t("settings.taskbarClock.align")}>
              <Segmented<ClockAlign>
                label={t("settings.taskbarClock.align")}
                value={settings.taskbarClockAlign}
                onChange={(taskbarClockAlign) => updateSettings({ taskbarClockAlign })}
                options={[
                  { value: "left", label: t("settings.taskbarClock.alignLeft") },
                  { value: "center", label: t("settings.taskbarClock.alignCenter") },
                  { value: "right", label: t("settings.taskbarClock.alignRight") },
                ]}
              />
            </Row>
            <Row title={t("settings.taskbarClock.preview")} hint={t("settings.taskbarClock.previewHint")}>
              <span className={styles.clockPreview} style={{ textAlign: settings.taskbarClockAlign }}>
                {preview}
              </span>
            </Row>
          </section>

          <h2 className={styles.groupTitle}>{t("settings.taskbarClock.weatherTitle")}</h2>
          <section className={styles.group}>
            <CityPicker />
            <Row title={t("settings.taskbarClock.unit")}>
              <Segmented<"c" | "f">
                label={t("settings.taskbarClock.unit")}
                value={settings.taskbarClockFahrenheit ? "f" : "c"}
                onChange={(unit) => updateSettings({ taskbarClockFahrenheit: unit === "f" })}
                options={[
                  { value: "c", label: "°C" },
                  { value: "f", label: "°F" },
                ]}
              />
            </Row>
          </section>
          <p className={styles.paneNote}>{t("settings.taskbarClock.weatherNote")}</p>

          <h2 className={styles.groupTitle}>{t("settings.taskbarClock.referenceTitle")}</h2>
          <p className={styles.paneHint}>{t("settings.taskbarClock.referenceHint", { number: target + 1 })}</p>
          {groups.map((group) => (
            <Fragment key={group}>
              <h3 className={styles.subTitle}>{t(`settings.taskbarClock.groups.${group}`)}</h3>
              <section className={styles.group}>
                {reference
                  .filter((row) => row.group === group)
                  .map((row) => (
                    <div key={row.id} className={styles.row}>
                      <button
                        type="button"
                        className={styles.token}
                        title={t("settings.taskbarClock.add", { number: target + 1 })}
                        onClick={() => addToken(row.template)}
                      >
                        {row.template}
                      </button>
                      <div className={styles.rowText}>
                        <span className={styles.rowHint}>{t(`settings.taskbarClock.tokens.${row.id}`)}</span>
                      </div>
                      <span className={styles.example}>{row.example}</span>
                    </div>
                  ))}
              </section>
            </Fragment>
          ))}
        </>
      )}
    </>
  );
}
