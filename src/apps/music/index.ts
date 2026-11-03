import { Music2 } from "lucide-react";
import { useMemo } from "react";
import type { Activity, DuriApp } from "../types";
import { startVolume } from "../volume/store";
import { startMusic, useMusic } from "./store";
import { MusicCompact, MusicExpanded, MusicMinimal, MusicPill } from "./views";

const PLAYING: Activity = { priority: 40 };

export const musicApp: DuriApp = {
  id: "music",
  nameKey: "apps.music.name",
  descriptionKey: "apps.music.description",
  Icon: Music2,
  accent: "var(--accent-music)",
  start() {
    startMusic();
    startVolume();
  },
  useActivity() {
    const { state } = useMusic();
    const active = state.available && state.playing;
    return useMemo(() => (active ? PLAYING : null), [active]);
  },
  useBackdrop() {
    const { state, artwork, accent } = useMusic();
    if (!state.available || !artwork) return null;
    // Album color glowing in from the top-left corner, fading into black.
    return `radial-gradient(130% 150% at 0% 0%, color-mix(in srgb, ${accent} 52%, black) 0%, color-mix(in srgb, ${accent} 20%, black) 42%, #000 78%)`;
  },
  Pill: MusicPill,
  pillWidth: 200,
  Minimal: MusicMinimal,
  Compact: MusicCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: MusicExpanded,
};
