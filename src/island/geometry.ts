import type { Size } from "../apps/types";
import type { IdleInfo, IslandStyle, PillSize, Transparency } from "../lib/settings";

/** Keep in sync with WIDTH/HEIGHT in src-tauri/src/island_window.rs. */
export const WINDOW_WIDTH = 640;
export const WINDOW_HEIGHT = 320;
/** Gap between the top of the screen and the island. */
export const TOP_INSET = 6;

export const PILL_HEIGHT = 34;
export const EMPTY_PILL: Size = { w: 124, h: PILL_HEIGHT };
/** Idle pill width for each Settings › Island › Idle pill option. */
export const IDLE_PILL: Record<IdleInfo, Size> = {
  none: EMPTY_PILL,
  clock: { w: 150, h: PILL_HEIGHT },
  stats: { w: 166, h: PILL_HEIGHT },
  both: { w: 222, h: PILL_HEIGHT },
};
/** A 12-hour time ("3:04 PM") is wider than "15:04". */
export const CLOCK_12H_EXTRA = 24;

/** Idle pill size for the idle info shown, with room for a 12-hour clock. */
export function idlePill(info: IdleInfo, hour12: boolean): Size {
  const base = IDLE_PILL[info] ?? EMPTY_PILL;
  return hour12 && (info === "clock" || info === "both") ? { w: base.w + CLOCK_12H_EXTRA, h: base.h } : base;
}
export const MINIMAL_SIZE = 34;
/** Default width of each end of an activity pill when idle info sits between them. */
export const PILL_SIDE = 48;
export const MINIMAL_GAP = 8;
export const COMPACT_DEFAULT: Size = { w: 372, h: 72 };
/** Clock + launcher; wide enough for every app icon plus Settings. */
export const HOME_SIZE: Size = { w: 500, h: 72 };
/** Height of the shared app dock at the bottom of the expanded island. */
export const EXPANDED_DOCK = 44;
/**
 * Every app in the dock opens at this size, so switching apps doesn't
 * resize the island and the dock stays put under the pointer.
 */
export const EXPANDED_SIZE: Size = { w: 460, h: 300 };
/** Inner padding of compact/expanded content; inner corner radius = outer − this. */
export const CONTENT_PADDING = 14;

export const HOVER_DELAY_MS = 90;
export const LEAVE_DELAY_MS = 260;
export const LEAVE_DELAY_EXPANDED_MS = 520;

export type Mode = "idle" | "compact" | "expanded";

/** Idle pill scale per Settings › General › Pill size. */
export const PILL_SCALE: Record<PillSize, number> = { compact: 0.85, default: 1, large: 1.2 };

export function topInset(style: IslandStyle) {
  return style === "attached" ? 0 : TOP_INSET;
}

export function cornerRadius(mode: Mode, size: Size): number {
  if (mode === "idle") return size.h / 2;
  if (mode === "compact") return Math.min(30, size.h / 2);
  return 34;
}

export interface Corners {
  borderTopLeftRadius: number;
  borderTopRightRadius: number;
  borderBottomRightRadius: number;
  borderBottomLeftRadius: number;
}

/** Attached islands hang from the screen edge, so their top corners are square. */
export function corners(mode: Mode, size: Size, style: IslandStyle): Corners {
  const r = cornerRadius(mode, size);
  const top = style === "attached" ? 0 : r;
  return { borderTopLeftRadius: top, borderTopRightRadius: top, borderBottomRightRadius: r, borderBottomLeftRadius: r };
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function islandRect(size: Size, top: number): Rect {
  return { x: (WINDOW_WIDTH - size.w) / 2, y: top, w: size.w, h: size.h };
}

export function minimalRect(main: Size, top: number, side: number): Rect {
  return { x: (WINDOW_WIDTH + main.w) / 2 + MINIMAL_GAP, y: top, w: side, h: side };
}

export function inflate(r: Rect, by: number): Rect {
  return { x: r.x - by, y: r.y - by, w: r.w + by * 2, h: r.h + by * 2 };
}

/**
 * Island background opacity per Transparency setting. The lowest still keeps
 * white text near 6.7:1 on a white desktop (black at 64% over #fff ≈ #5c5c5c).
 */
export const ISLAND_ALPHA: Record<Transparency, number> = { off: 1, low: 0.82, high: 0.64 };

/** The tucked-away island: a short bar at the top edge. */
export const HANDLE: Size = { w: 48, h: 6 };
/** Padding around the handle's hit region (HIG: ~24 pt around elements without a bezel). */
const HANDLE_PAD = 24;
/** How far the island must be dragged (px) to tuck it away or bring it back. */
export const TUCK_DISTANCE = 24;

/** Generous hover region for the handle, reaching up to the screen edge. */
export function handleHitRect(top: number): Rect {
  return {
    x: (WINDOW_WIDTH - HANDLE.w) / 2 - HANDLE_PAD,
    y: 0,
    w: HANDLE.w + HANDLE_PAD * 2,
    h: top + HANDLE.h + HANDLE_PAD / 2,
  };
}

/**
 * Auto-hide: the strip at the very top of the screen that brings the island
 * back, like the edge an auto-hiding taskbar watches. A few px tall and as
 * wide as a pill, so it doesn't catch the pointer on its way to browser tabs.
 */
export function revealStripRect(width = 240): Rect {
  return { x: (WINDOW_WIDTH - width) / 2, y: 0, w: width, h: 3 };
}

/**
 * What letting go of a vertical drag of `dy` px does: up tucks, down brings
 * back. The island sits at the top of the screen, so the pointer often hits
 * the edge before travelling far: reaching the edge counts as "up" too.
 */
export function tuckGesture(dy: number, tucked: boolean, atTopEdge = false): "tuck" | "untuck" | null {
  if (!tucked && (dy <= -TUCK_DISTANCE || (atTopEdge && dy <= -6))) return "tuck";
  if (tucked && dy >= TUCK_DISTANCE) return "untuck";
  return null;
}

/** Visual offset while dragging: follows the pointer with resistance, then stops. */
export function dragOffset(dy: number, tucked: boolean): number {
  const toward = tucked ? Math.max(0, dy) : Math.min(0, dy);
  const away = tucked ? Math.min(0, dy) : Math.max(0, dy);
  return Math.max(-TUCK_DISTANCE, Math.min(TUCK_DISTANCE, toward * 0.6)) + away * 0.15;
}
