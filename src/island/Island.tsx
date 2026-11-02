import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion, useAnimate, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { rankActivities, sameActivity } from "../apps/activity";
import { registry } from "../apps/registry";
import type { Activity, DuriApp, Size } from "../apps/types";
import { contentIn, contentOut, morphSpring, reducedTransition, settleSpring } from "../design/motion";
import { useSettings } from "../lib/settings";
import { IslandContext, setIslandFocus, type IslandControls } from "./context";
import { ExpandedChrome } from "./ExpandedChrome";
import {
  COMPACT_DEFAULT,
  EMPTY_PILL,
  HOME_SIZE,
  HOVER_DELAY_MS,
  LEAVE_DELAY_EXPANDED_MS,
  LEAVE_DELAY_MS,
  MINIMAL_GAP,
  PILL_HEIGHT,
  TOP_INSET,
  cornerRadius,
  inflate,
  islandRect,
  minimalRect,
} from "./geometry";
import { Home } from "./Home";
import styles from "./Island.module.css";
import { initialIslandState, islandReducer, type IslandState } from "./state";

type View =
  | { kind: "empty" }
  | { kind: "pill"; app: DuriApp }
  | { kind: "home" }
  | { kind: "compact"; app: DuriApp }
  | { kind: "expanded"; app: DuriApp };

function sizeOf(view: View): Size {
  switch (view.kind) {
    case "empty":
      return EMPTY_PILL;
    case "pill":
      return { w: view.app.pillWidth, h: PILL_HEIGHT };
    case "home":
      return HOME_SIZE;
    case "compact":
      return view.app.compactSize ?? COMPACT_DEFAULT;
    case "expanded":
      return view.app.expandedSize;
  }
}

function resolveView(
  mode: IslandState["mode"],
  appId: string | null,
  attention: string | null,
  primary: DuriApp | undefined,
  apps: DuriApp[],
): View {
  const byId = (id: string | null) => (id ? apps.find((a) => a.id === id) : undefined);
  if (mode === "expanded") {
    const app = byId(appId) ?? primary ?? apps[0];
    return app ? { kind: "expanded", app } : { kind: "home" };
  }
  if (mode === "compact") {
    const app = byId(attention) ?? primary;
    return app ? { kind: "compact", app } : { kind: "home" };
  }
  return primary ? { kind: "pill", app: primary } : { kind: "empty" };
}

const noBackdrop = () => null;

interface ProbeProps {
  app: DuriApp;
  report: (id: string, a: Activity | null) => void;
  reportBackdrop: (id: string, b: string | null) => void;
}

/** Mounts each app's hooks in its own component (hooks can't run in a loop). */
function AppProbe({ app, report, reportBackdrop }: ProbeProps) {
  const activity = app.useActivity();
  const useBackdrop = app.useBackdrop ?? noBackdrop;
  const backdrop = useBackdrop();
  useEffect(() => report(app.id, activity), [app.id, activity, report]);
  useEffect(() => reportBackdrop(app.id, backdrop), [app.id, backdrop, reportBackdrop]);
  return null;
}

const openSettings = () => invoke("open_settings_window").catch(() => {});

export function Island() {
  const { t } = useTranslation();
  const reduceMotion = useReducedMotion();
  const settings = useSettings();
  const apps = useMemo(
    () => registry.filter((a) => !settings.disabledApps.includes(a.id)),
    [settings.disabledApps],
  );

  useEffect(() => registry.forEach((a) => a.start?.()), []);

  const [activities, setActivities] = useState<Record<string, Activity | null>>({});
  const report = useCallback(
    (id: string, a: Activity | null) =>
      setActivities((prev) => (sameActivity(prev[id], a) ? prev : { ...prev, [id]: a })),
    [],
  );
  const [backdrops, setBackdrops] = useState<Record<string, string | null>>({});
  const reportBackdrop = useCallback(
    (id: string, b: string | null) => setBackdrops((prev) => (prev[id] === b ? prev : { ...prev, [id]: b })),
    [],
  );
  const ranked = rankActivities(apps, activities);
  const primary = ranked[0];
  const secondary = ranked[1];

  const [state, dispatch] = useReducer(islandReducer, initialIslandState);

  useEffect(() => {
    const un = listen<boolean>("island://pointer", (e) => dispatch({ type: "pointer", inside: e.payload }));
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Debounce hover/leave so brushing past the island doesn't open it.
  useEffect(() => {
    const inside = state.pointerInside;
    const delay = inside ? HOVER_DELAY_MS : state.mode === "expanded" ? LEAVE_DELAY_EXPANDED_MS : LEAVE_DELAY_MS;
    const id = setTimeout(() => dispatch({ type: inside ? "hover" : "leave" }), delay);
    return () => clearTimeout(id);
  }, [state.pointerInside, state.mode, state.locked]);

  // Give keyboard focus back whenever the island rests, and let Esc close it.
  useEffect(() => {
    if (state.mode === "idle") setIslandFocus(false);
  }, [state.mode]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dispatch({ type: "collapse" });
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Leaving typing mode waits a moment: taking focus makes the field blur and
  // refocus briefly, and reacting to that blur would hand the keystrokes
  // that follow to another app.
  const releaseTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const controls = useMemo<IslandControls>(
    () => ({
      collapse: () => dispatch({ type: "collapse" }),
      setTyping: (typing) => {
        clearTimeout(releaseTimer.current);
        if (typing) {
          dispatch({ type: "lock", locked: true });
          setIslandFocus(true);
          return;
        }
        releaseTimer.current = setTimeout(() => {
          const el = document.activeElement;
          if (document.hasFocus() && el instanceof HTMLElement && el.matches("input, textarea")) return;
          dispatch({ type: "lock", locked: false });
          setIslandFocus(false);
        }, 150);
      },
    }),
    [],
  );

  // Alerts hold the island open and bounce it once.
  const alert = ranked.find((r) => r.activity.alert);
  const alertKey = alert ? `${alert.app.id}:${alert.activity.key ?? ""}` : null;
  const alertAppId = alert?.app.id ?? null;
  const [scope, animate] = useAnimate();
  useEffect(() => {
    if (alertAppId) {
      dispatch({ type: "attention", appId: alertAppId });
      if (!reduceMotion && scope.current) {
        animate(scope.current, { scale: [1, 1.08, 0.97, 1] }, { duration: 0.6, ease: "easeOut" });
      }
    } else {
      dispatch({ type: "attentionEnd" });
    }
  }, [alertKey, alertAppId, reduceMotion, animate, scope]);

  const view = resolveView(state.mode, state.appId, state.attention, primary?.app, apps);

  const size = sizeOf(view);
  const mode = state.mode;
  const showMinimal = mode === "idle" && !!secondary;
  const viewKey = `${view.kind}:${"app" in view ? view.app.id : ""}`;
  const backdrop = view.kind === "compact" || view.kind === "expanded" ? (backdrops[view.app.id] ?? null) : null;

  // Tell the backend which parts of the window are clickable.
  useEffect(() => {
    const rects = [inflate(islandRect(size), 2)];
    if (showMinimal) rects.push(minimalRect(size));
    invoke("island_set_hit_rects", { rects }).catch(() => {});
  }, [size.w, size.h, showMinimal]); // eslint-disable-line react-hooks/exhaustive-deps

  const transition = reduceMotion ? reducedTransition : state.growing ? morphSpring : settleSpring;

  const open = (appId: string) => dispatch({ type: "open", appId });
  const onIslandClick = () => {
    if (mode === "expanded") return;
    const target = "app" in view ? view.app.id : apps[0]?.id;
    if (target) open(target);
  };

  let content: React.ReactNode = null;
  switch (view.kind) {
    case "pill":
      content = <view.app.Pill />;
      break;
    case "home":
      content = <Home apps={apps} onOpen={open} onSettings={openSettings} />;
      break;
    case "compact":
      content = <view.app.Compact />;
      break;
    case "expanded":
      content = (
        <ExpandedChrome apps={apps} current={view.app} onSelect={open} onSettings={openSettings}>
          <view.app.Expanded />
        </ExpandedChrome>
      );
      break;
  }

  return (
    <IslandContext.Provider value={controls}>
    <div className={styles.stage} style={{ "--top": `${TOP_INSET}px` } as CSSProperties}>
      {apps.map((app) => (
        <AppProbe key={app.id} app={app} report={report} reportBackdrop={reportBackdrop} />
      ))}

      <div ref={scope} className={styles.anchor}>
        <motion.div
          className={styles.island}
          data-mode={mode}
          role="region"
          aria-label={t("island.label")}
          initial={false}
          animate={{ width: size.w, height: size.h, borderRadius: cornerRadius(mode, size) }}
          transition={transition}
          onClick={onIslandClick}
        >
          <div
            className={styles.backdrop}
            style={{ backgroundImage: backdrop ?? undefined, opacity: backdrop ? 1 : 0 }}
            aria-hidden
          />
          <AnimatePresence initial={false}>
            <motion.div
              key={viewKey}
              className={styles.layer}
              style={{ width: size.w, height: size.h }}
              initial={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, filter: "blur(6px)" }}
              animate={{ opacity: 1, scale: 1, filter: "blur(0px)", transition: contentIn }}
              exit={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, filter: "blur(6px)", transition: contentOut }}
            >
              {content}
            </motion.div>
          </AnimatePresence>
        </motion.div>
      </div>

      <AnimatePresence>
        {showMinimal && secondary && (
          <motion.button
            key={secondary.app.id}
            type="button"
            className={styles.minimal}
            style={{ left: `calc(50% + ${size.w / 2 + MINIMAL_GAP}px)` }}
            aria-label={t("island.switchTo", { name: t(secondary.app.nameKey) })}
            initial={{ opacity: 0, scale: 0.3, x: -24 }}
            animate={{ opacity: 1, scale: 1, x: 0 }}
            exit={{ opacity: 0, scale: 0.3, x: -24 }}
            transition={reduceMotion ? reducedTransition : morphSpring}
            onClick={() => open(secondary.app.id)}
          >
            <secondary.app.Minimal />
          </motion.button>
        )}
      </AnimatePresence>
    </div>
    </IslandContext.Provider>
  );
}
