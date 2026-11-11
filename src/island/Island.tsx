import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion, useAnimate, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { rankActivities, sameActivity } from "../apps/activity";
import { registry } from "../apps/registry";
import type { Activity, DuriApp, Size } from "../apps/types";
import { contentIn, contentOut, morphSpring, reducedTransition, settleSpring } from "../design/motion";
import { useHour12 } from "../lib/clock";
import { requestSearchFocus } from "../apps/clipboard/store";
import { notesActions } from "../apps/notes/store";
import { updateSettings, useSettings } from "../lib/settings";
import type { ShortcutAction } from "../lib/shortcuts";
import { startSystemStats } from "../lib/systemStats";
import { startUpdateChecks } from "../lib/updates";
import { IslandContext, setIslandFocus, type IslandControls } from "./context";
import { ExpandedChrome } from "./ExpandedChrome";
import {
  COMPACT_DEFAULT,
  EMPTY_PILL,
  EXPANDED_SIZE,
  HOME_SIZE,
  PILL_SIDE,
  HOVER_DELAY_MS,
  LEAVE_DELAY_EXPANDED_MS,
  LEAVE_DELAY_MS,
  MINIMAL_GAP,
  MINIMAL_SIZE,
  PILL_HEIGHT,
  PILL_SCALE,
  HANDLE,
  WINDOW_HEIGHT,
  WINDOW_WIDTH,
  corners,
  dragOffset,
  handleHitRect,
  revealStripRect,
  idlePill,
  tuckGesture,
  ISLAND_ALPHA,
  inflate,
  islandRect,
  minimalRect,
  topInset,
} from "./geometry";
import { Home } from "./Home";
import { IdleInfo } from "./IdleInfo";
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
      return view.app.hidden ? (view.app.expandedSize ?? EXPANDED_SIZE) : EXPANDED_SIZE;
  }
}

function resolveView(
  mode: IslandState["mode"],
  appId: string | null,
  attention: string | null,
  peek: string | null,
  primary: DuriApp | undefined,
  apps: DuriApp[],
): View {
  const byId = (id: string | null) => (id ? apps.find((a) => a.id === id) : undefined);
  if (mode === "expanded") {
    const app = byId(appId) ?? primary ?? apps[0];
    return app ? { kind: "expanded", app } : { kind: "home" };
  }
  if (mode === "compact") {
    const app = byId(attention) ?? byId(peek);
    return app ? { kind: "compact", app } : { kind: "home" };
  }
  return primary ? { kind: "pill", app: primary } : { kind: "empty" };
}

const viewKeyOf = (v: View) => `${v.kind}:${"app" in v ? v.app.id : ""}`;

/**
 * While the island is open in compact form, switching to another app's
 * compact view waits until the change has held for a moment. Activity can
 * flicker (a session going working → done → working), and every switch
 * cross-fades the content; without this the text visibly blinks.
 */
function useSteadyView(view: View): View {
  const key = viewKeyOf(view);
  const [shown, setShown] = useState({ key, view });
  const latest = useRef(view);
  useEffect(() => {
    latest.current = view;
  });
  const hold = shown.key !== key && shown.view.kind === "compact" && view.kind === "compact";
  if (shown.key !== key && !hold) setShown({ key, view });
  useEffect(() => {
    if (!hold) return;
    const id = setTimeout(() => setShown({ key, view: latest.current }), STEADY_MS);
    return () => clearTimeout(id);
  }, [hold, key]);
  return hold ? shown.view : view;
}

const STEADY_MS = 180;

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
  const hour12 = useHour12();
  const apps = useMemo(
    () => registry.filter((a) => !settings.disabledApps.includes(a.id)),
    [settings.disabledApps],
  );

  useEffect(() => {
    registry.forEach((a) => a.start?.());
    startSystemStats();
    startUpdateChecks();
  }, []);

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
  // Read by the hover timer, so it peeks at whatever is on top at that moment.
  const primaryRef = useRef<string | null>(null);
  useEffect(() => {
    primaryRef.current = primary?.app.id ?? null;
  });

  useEffect(() => {
    const un = listen<boolean>("island://pointer", (e) => dispatch({ type: "pointer", inside: e.payload }));
    invoke<boolean>("island_pointer").then((inside) => dispatch({ type: "pointer", inside })).catch(() => {});
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Clicking the island brings it to the front; give focus back to the
  // previous app as soon as it rests again (pointer left, Esc, outside click).
  useEffect(() => {
    if (state.mode !== "expanded" && !state.locked) setIslandFocus(false);
  }, [state.mode, state.locked]);
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

  // Global shortcuts open the island from the keyboard. It stays open (no
  // pointer to keep it there) until Esc, or until another app takes focus.
  const keyboardOpen = useRef(false);
  const latest = useRef({ mode: state.mode, primary: primaryRef, apps });
  useEffect(() => {
    latest.current = { mode: state.mode, primary: primaryRef, apps };
  });
  useEffect(() => {
    const has = (id: string) => latest.current.apps.some((a) => a.id === id);
    const openFromKeyboard = (appId: string) => {
      keyboardOpen.current = true;
      dispatch({ type: "open", appId });
      dispatch({ type: "lock", locked: true });
      setIslandFocus(true);
    };
    const un = listen<ShortcutAction>("island://shortcut", (e) => {
      const { mode, primary, apps } = latest.current;
      if (e.payload === "island") {
        if (mode === "expanded") {
          dispatch({ type: "collapse" });
          return;
        }
        const top = apps.find((a) => a.id === primary.current);
        const target = top && !top.noExpand ? top.id : apps.find((a) => !a.hidden)?.id;
        if (target) openFromKeyboard(target);
      } else if (e.payload === "note" && has("notes")) {
        notesActions.create();
        openFromKeyboard("notes");
      } else if (e.payload === "clipboard" && has("clipboard")) {
        requestSearchFocus();
        openFromKeyboard("clipboard");
      }
    });
    const onBlur = () => {
      if (!keyboardOpen.current) return;
      keyboardOpen.current = false;
      dispatch({ type: "collapse" });
    };
    window.addEventListener("blur", onBlur);
    return () => {
      un.then((f) => f());
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  useEffect(() => {
    if (state.mode === "idle") keyboardOpen.current = false;
  }, [state.mode]);

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

  const resolved = resolveView(state.mode, state.appId, state.attention, state.peek, primary?.app, apps);
  const view = useSteadyView(resolved);

  // Tuck away: dragging the island up shrinks it to a handle at the top
  // edge; hovering the handle shows the island again until the pointer
  // leaves, and dragging it down (or the right-click menu) brings it back.
  // Alerts still show while tucked.
  const [dragY, setDragY] = useState<number | null>(null);
  const dragStart = useRef<number | null>(null);
  // The button is down on the island (a click or the start of a drag).
  const [pressed, setPressed] = useState(false);
  const suppressClick = useRef(false);
  const tucked = settings.tucked;
  // Volume and lock key indicators show through tucking, auto-hide and
  // fullscreen apps, like the Windows popups they stand in for.
  const hudActive = ranked.some((r) => r.activity.hud);
  // The pointer reaching the handle doesn't count as leaving rest: the
  // island would flash its pill before the hover opens it.
  const resting = state.mode === "idle" && !alertKey && !hudActive && dragY === null;
  // Auto-hide goes one step further than tucking: nothing shows at rest.
  const away = settings.autoHide && resting;
  const handle = (tucked || settings.autoHide) && resting;

  // Debounce hover/leave so brushing past the island doesn't open it. The
  // auto-hide strip sits at the very edge of the screen, so reaching it is
  // deliberate: it opens at once, like an auto-hiding taskbar.
  useEffect(() => {
    const inside = state.pointerInside;
    const delay = inside
      ? away
        ? 0
        : HOVER_DELAY_MS
      : state.mode === "expanded"
        ? LEAVE_DELAY_EXPANDED_MS
        : LEAVE_DELAY_MS;
    const id = setTimeout(
      () => dispatch(inside ? { type: "hover", appId: primaryRef.current } : { type: "leave" }),
      delay,
    );
    return () => clearTimeout(id);
  }, [state.pointerInside, state.mode, state.locked]); // eslint-disable-line react-hooks/exhaustive-deps

  // Lets the backend show the island over a fullscreen app while an
  // indicator is up. Turning it off waits for the pill to settle.
  useEffect(() => {
    if (hudActive) {
      invoke("island_set_hud", { on: true }).catch(() => {});
      return;
    }
    const id = setTimeout(() => invoke("island_set_hud", { on: false }).catch(() => {}), 350);
    return () => clearTimeout(id);
  }, [hudActive]);

  // Activity pills show the idle info (clock, CPU…) in the middle, between
  // the app's content at both ends, so the pill has no empty gap.
  const centerInfo =
    view.kind === "pill" && view.app.pillCenter !== false && settings.idleInfo !== "none" ? settings.idleInfo : null;

  // The idle pill (and its bubble) scale with the Pill size setting; the
  // content is drawn at base size and zoomed so it stays crisp.
  const base =
    view.kind === "empty"
      ? idlePill(settings.idleInfo, hour12)
      : centerInfo && view.kind === "pill"
        ? { w: idlePill(centerInfo, hour12).w + 2 * (view.app.pillSide ?? PILL_SIDE), h: PILL_HEIGHT }
        : sizeOf(view);
  const zoom = view.kind === "pill" || view.kind === "empty" ? PILL_SCALE[settings.pillSize] ?? 1 : 1;
  const size = handle ? HANDLE : { w: Math.round(base.w * zoom), h: Math.round(base.h * zoom) };
  const bubbleZoom = PILL_SCALE[settings.pillSize] ?? 1;
  const bubble = Math.round(MINIMAL_SIZE * bubbleZoom);
  const attached = settings.islandStyle === "attached";
  const top = topInset(settings.islandStyle);
  const mode = state.mode;
  const showMinimal = mode === "idle" && !!secondary && !handle;
  const viewKey = handle ? "handle" : `${view.kind}:${"app" in view ? view.app.id : ""}`;
  const alpha = ISLAND_ALPHA[settings.transparency] ?? 1;
  const backdrop = view.kind === "compact" || view.kind === "expanded" ? (backdrops[view.app.id] ?? null) : null;

  // Tell the backend which parts of the window are clickable.
  // Drops a press without acting on it.
  function cancelPress() {
    dragStart.current = null;
    setPressed(false);
    setDragY(null);
  }
  // A release the island itself didn't get (a child stopped it, or the
  // window lost focus) must still end the press, or the whole window would
  // keep swallowing clicks.
  useEffect(() => {
    if (!pressed) return;
    const end = () => {
      if (dragStart.current === null) setPressed(false);
      else cancelPress();
    };
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
    };
  }, [pressed]);

  // While the button is down, the whole window takes the pointer: otherwise
  // a drag that leaves the island before it passes the drag threshold makes
  // the window click-through, and the move and the release are lost.
  const dragging = dragY !== null;
  const holding = dragging || pressed;
  useEffect(() => {
    const rects = holding
      ? [{ x: 0, y: 0, w: WINDOW_WIDTH, h: WINDOW_HEIGHT }]
      : away
        ? [revealStripRect()]
        : handle
          ? [handleHitRect(top)]
        : [inflate(islandRect(size, top), 2)];
    if (showMinimal && !holding) rects.push(minimalRect(size, top, bubble));
    invoke("island_set_hit_rects", { rects }).catch(() => {});
  }, [size.w, size.h, showMinimal, top, bubble, handle, away, holding]); // eslint-disable-line react-hooks/exhaustive-deps

  const transition = reduceMotion ? reducedTransition : state.growing ? morphSpring : settleSpring;

  const open = (appId: string) => {
    if (apps.find((a) => a.id === appId)?.noExpand) return;
    dispatch({ type: "open", appId });
  };
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (mode === "expanded" || e.button !== 0) return;
    dragStart.current = e.clientY;
    setPressed(true);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragStart.current === null) return;
    // The release happened somewhere we didn't see it: don't let a plain
    // hover carry on a stale drag.
    if ((e.buttons & 1) === 0) {
      cancelPress();
      return;
    }
    const dy = e.clientY - dragStart.current;
    if (dragY === null && Math.abs(dy) <= 4) return;
    // Capture only once it is really a drag: capturing on press would send
    // the click to the island instead of the button under the pointer.
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.setPointerCapture(e.pointerId);
    setDragY(dy);
  };
  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragStart.current === null) return;
    dragStart.current = null;
    setPressed(false);
    if (dragY === null) return;
    suppressClick.current = true;
    // The window starts at the top of the screen, so clientY ≈ 0 is the edge.
    const action = tuckGesture(dragY, tucked, e.clientY <= 1);
    setDragY(null);
    if (action) {
      updateSettings({ tucked: action === "tuck" }).catch(() => {});
      dispatch({ type: "collapse" });
    }
  };

  const onIslandClick = () => {
    if (suppressClick.current) {
      suppressClick.current = false;
      return;
    }
    if (mode === "expanded") return;
    const target = "app" in view ? view.app.id : apps[0]?.id;
    if (target) open(target);
  };

  let content: React.ReactNode = null;
  if (!handle) switch (view.kind) {
    case "empty":
      content = <IdleInfo mode={settings.idleInfo} />;
      break;
    case "pill":
      content = (
        <>
          <view.app.Pill />
          {centerInfo && (
            <div className={styles.pillCenter}>
              <IdleInfo mode={centerInfo} />
            </div>
          )}
        </>
      );
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
    <div className={styles.stage} data-style={settings.islandStyle} style={{ "--top": `${top}px`, "--island-bg": `rgba(0, 0, 0, ${alpha})` } as CSSProperties}>
      {apps.map((app) => (
        <AppProbe key={app.id} app={app} report={report} reportBackdrop={reportBackdrop} />
      ))}

      <div ref={scope} className={styles.anchor}>
        {attached && !handle && (
          <>
            <span className={`${styles.fillet} ${styles.filletLeft}`} aria-hidden />
            <span className={`${styles.fillet} ${styles.filletRight}`} aria-hidden />
          </>
        )}
        <motion.div
          className={styles.island}
          data-mode={mode}
          data-handle={handle}
          data-away={away}
          role="region"
          aria-label={handle ? t("island.handle") : t("island.label")}
          initial={false}
          animate={{ width: size.w, height: size.h, ...corners(mode, size, settings.islandStyle) }}
          style={{ y: dragging ? dragOffset(dragY, tucked) : 0 }}
          transition={transition}
          onClick={onIslandClick}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          <div
            className={styles.backdrop}
            style={{ backgroundImage: backdrop ?? undefined, opacity: backdrop ? alpha : 0 }}
            aria-hidden
          />
          <AnimatePresence initial={false}>
            {!handle && (
            <motion.div
              key={viewKey}
              className={styles.layer}
              style={{ width: base.w, height: base.h, zoom }}
              initial={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.97 }}
              animate={{ opacity: 1, scale: 1, transition: contentIn }}
              exit={{ opacity: 0, transition: contentOut }}
            >
              {content}
            </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>

      <AnimatePresence>
        {showMinimal && secondary && (
          <motion.button
            key={secondary.app.id}
            type="button"
            className={styles.minimal}
            style={{ left: `calc(50% + ${size.w / 2 + MINIMAL_GAP}px)`, width: bubble, height: bubble }}
            aria-label={t("island.switchTo", { name: t(secondary.app.nameKey) })}
            initial={{ opacity: 0, scale: 0.3, x: -24 }}
            animate={{ opacity: 1, scale: 1, x: 0 }}
            exit={{ opacity: 0, scale: 0.3, x: -24 }}
            transition={reduceMotion ? reducedTransition : morphSpring}
            onClick={() => open(secondary.app.id)}
          >
            <span style={{ zoom: bubbleZoom, display: "grid", placeItems: "center" }}>
              <secondary.app.Minimal />
            </span>
          </motion.button>
        )}
      </AnimatePresence>
    </div>
    </IslandContext.Provider>
  );
}
