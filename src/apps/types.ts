import type { ComponentType } from "react";

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
  Minimal: ComponentType;
  Compact: ComponentType;
  compactSize?: Size;
  Expanded: ComponentType;
  /**
   * Full island size when expanded. The bottom `EXPANDED_DOCK` px hold the
   * shared app dock; the body gets the rest minus `CONTENT_PADDING`.
   */
  expandedSize: Size;
}
