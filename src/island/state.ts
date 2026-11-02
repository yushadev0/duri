import type { Mode } from "./geometry";

export interface IslandState {
  mode: Mode;
  /** App shown when expanded. */
  appId: string | null;
  /** App holding the island open with an alert (e.g. finished timer). */
  attention: string | null;
  /** Raw pointer state from the backend hit test (not debounced). */
  pointerInside: boolean;
  /** Whether the last mode change made the island bigger (picks the spring). */
  growing: boolean;
  /** Held open while someone types in it; pointer leaving doesn't collapse. */
  locked: boolean;
}

export type IslandAction =
  | { type: "pointer"; inside: boolean }
  /** Debounced: pointer has rested on the island. */
  | { type: "hover" }
  /** Debounced: pointer has left the island. */
  | { type: "leave" }
  | { type: "open"; appId: string }
  | { type: "collapse" }
  | { type: "attention"; appId: string }
  | { type: "attentionEnd" }
  | { type: "lock"; locked: boolean };

export const initialIslandState: IslandState = {
  mode: "idle",
  appId: null,
  attention: null,
  pointerInside: false,
  growing: true,
  locked: false,
};

const rank: Record<Mode, number> = { idle: 0, compact: 1, expanded: 2 };

function withMode(s: IslandState, mode: Mode, patch: Partial<IslandState> = {}): IslandState {
  return { ...s, ...patch, mode, growing: rank[mode] >= rank[s.mode] };
}

/** Where the island rests when nobody is interacting with it. */
function restingMode(s: IslandState): Mode {
  return s.attention ? "compact" : "idle";
}

export function islandReducer(s: IslandState, a: IslandAction): IslandState {
  switch (a.type) {
    case "pointer":
      return s.pointerInside === a.inside ? s : { ...s, pointerInside: a.inside };
    case "hover":
      return s.mode === "idle" && s.pointerInside ? withMode(s, "compact") : s;
    case "leave":
      if (s.pointerInside || s.locked) return s;
      return s.mode === restingMode(s) ? s : withMode(s, restingMode(s));
    case "open":
      return withMode(s, "expanded", { appId: a.appId });
    case "collapse":
      return withMode(s, restingMode(s), { locked: false });
    case "lock":
      return s.locked === a.locked ? s : { ...s, locked: a.locked };
    case "attention":
      if (s.attention === a.appId) return s;
      return s.mode === "expanded"
        ? { ...s, attention: a.appId, appId: a.appId }
        : withMode(s, "compact", { attention: a.appId });
    case "attentionEnd": {
      if (!s.attention) return s;
      const cleared = { ...s, attention: null };
      return s.mode === "compact" && !s.pointerInside ? withMode(cleared, "idle") : cleared;
    }
  }
}
