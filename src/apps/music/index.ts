import { Music2 } from "lucide-react";
import { useMemo } from "react";
import type { Activity, DuriApp } from "../types";
import { startMusic, useMusic } from "./store";
import { MusicCompact, MusicExpanded, MusicMinimal, MusicPill } from "./views";

const PLAYING: Activity = { priority: 40 };

export const musicApp: DuriApp = {
  id: "music",
  nameKey: "apps.music.name",
  descriptionKey: "apps.music.description",
  Icon: Music2,
  accent: "var(--accent-music)",
  start: startMusic,
  useActivity() {
    const { state } = useMusic();
    const active = state.available && state.playing;
    return useMemo(() => (active ? PLAYING : null), [active]);
  },
  Pill: MusicPill,
  pillWidth: 200,
  Minimal: MusicMinimal,
  Compact: MusicCompact,
  compactSize: { w: 372, h: 72 },
  Expanded: MusicExpanded,
  expandedSize: { w: 440, h: 222 },
};
