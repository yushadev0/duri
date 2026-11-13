import type { ComponentType, ReactNode } from "react";

export interface Size {
  w: number;
  h: number;
}

/**
 * Something an app wants shown while the island is idle (music playing, a
 * timer running). The highest priority activity gets the pill; the runner-up
 * gets the small detached bubble.
 */
export interface Activity {
  priority: number;
  /** Forces the island open in compact form and bounces it (e.g. timer done). */
  alert?: boolean;
  /** Changes when a new alert should re-trigger attention. */
  key?: string;
  /**
   * A short-lived indicator (volume, Caps Lock): shows even while the island
   * is tucked away, auto-hidden or behind a fullscreen app.
   */
  hud?: boolean;
}

export interface IconProps {
  size?: number;
  className?: string;
}

/**
 * The contract every Duri app implements. See docs/ADDING_AN_APP.md.
 *
 * Presentations, from smallest to largest:
 * - `Minimal`: 34×34 bubble beside the pill when another app owns the pill.
 * - `Pill`: idle island while this app has the top activity (height 34).
 * - `Compact`: shown on hover.
 * - `Expanded`: shown on click; the island adds a header with the app switcher.
 */
export interface DuriApp {
  id: string;
  /** i18n keys */
  nameKey: string;
  descriptionKey: string;
  Icon: ComponentType<IconProps>;
  /** Identity color used for the icon tile in Settings and the switcher. */
  accent: string;
  /** System apps (e.g. Volume) that only appear on their own, never in the launcher or dock. */
  hidden?: boolean;
  /** Part of Duri itself (the update prompt): not listed in Settings › Apps, can't be turned off. */
  system?: boolean;
  /** Called once when the island starts, to begin listening to the backend. */
  start?: () => void;
  useActivity: () => Activity | null;
  /**
   * Optional CSS background painted behind the Compact and Expanded views
   * (the Pill always stays black). Return null for plain black.
   */
  useBackdrop?: () => string | null;
  Pill: ComponentType;
  pillWidth: number;
  /**
   * Optional: a different pill width while something needs the room (the
   * music pill with lyrics). Null keeps `pillWidth` (or the idle info width).
   * Keep it steady: it is a morph of the whole island each time it changes.
   */
  usePillWidth?: () => number | null;
  /**
   * Width of each end of the pill (content at both edges). The idle info
   * (clock, CPU…) goes in between; defaults to `PILL_SIDE`.
   */
  pillSide?: number;
  /** Set false for pills that use their full width (e.g. the volume bar). */
  pillCenter?: boolean;
  /**
   * Optional: what goes in the middle of the pill instead of the idle info
   * (the music pill shows the line being sung). Gets the idle info as
   * `fallback` (null when it is off) and the width of each end (`side`). The
   * pill keeps its size either way.
   */
  PillCenter?: ComponentType<{ fallback: ReactNode; side: number }>;
  Minimal: ComponentType;
  Compact: ComponentType;
  compactSize?: Size;
  Expanded: ComponentType;
  /**
   * Indicators whose hover view already says everything (Lock keys): clicks
   * on the pill, the bubble or the compact view don't expand them.
   */
  noExpand?: boolean;
  /**
   * Apps in the dock all open at `EXPANDED_SIZE` (460×300; the bottom
   * `EXPANDED_DOCK` px hold the dock, the body gets the rest minus
   * `CONTENT_PADDING`). Only `hidden` apps, which open on their own, can
   * pick a size here.
   */
  expandedSize?: Size;
}
