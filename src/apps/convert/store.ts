/**
 * Converter state: the backend's snapshot (ffmpeg, jobs) plus the files
 * dropped on the island that are waiting for a format ("staged").
 * Backend: src-tauri/src/apps/convert/.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createStore, useStore } from "../../lib/store";

export type Kind = "video" | "audio" | "image" | "animation";
export type Quality = "high" | "balanced" | "small";

export type Tools =
  | { state: "missing" }
  | { state: "downloading"; progress: number }
  | { state: "ready" }
  | { state: "failed"; error: string };

export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export interface Job {
  id: number;
  input: string;
  name: string;
  target: string;
  status: JobStatus;
  progress: number | null;
  output: string | null;
  error: string | null;
  startedAt: number | null;
}

export interface Snapshot {
  tools: Tools;
  jobs: Job[];
  packageBytes: number;
}

export interface Target {
  id: string;
  output: Kind;
  lossless: boolean;
}

export interface Media {
  kind: Kind | null;
  duration: number | null;
  width: number | null;
  height: number | null;
  hasAudio: boolean;
}

export interface Probed {
  path: string;
  name: string;
  bytes: number;
  media: Media;
  targets: Target[];
  error: string | null;
}

export interface Advanced {
  maxHeight?: number;
  maxWidth?: number;
  fps?: number;
  audioBitrate?: number;
  sampleRate?: number;
  mono?: boolean;
  mute?: boolean;
  start?: number;
  end?: number;
}

export interface Staged {
  paths: string[];
  /** Null while ffprobe looks at the files (or ffmpeg isn't there yet). */
  files: Probed[] | null;
  target: string | null;
  quality: Quality;
  advanced: Advanced;
  showAdvanced: boolean;
}

const snapshot = createStore<Snapshot>({ tools: { state: "missing" }, jobs: [], packageBytes: 0 });
const staged = createStore<Staged | null>(null);
/** Jobs that just finished: the island bounces and shows them for a moment. */
const fresh = createStore<number[]>([]);

export const useConvert = () => useStore(snapshot);
export const useStaged = () => useStore(staged);
export const useFresh = () => useStore(fresh);

/** How long a finished conversion holds the island open. */
const FRESH_MS = 8000;

// Last choices, per kind of file, so the next drop starts there.
const PREFS_KEY = "duri.convert.prefs";
interface Prefs {
  targets: Partial<Record<Kind, string>>;
  quality: Quality;
}
function loadPrefs(): Prefs {
  try {
    const p = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "null");
    if (p && typeof p === "object") return { targets: p.targets ?? {}, quality: p.quality ?? "balanced" };
  } catch {
    // Unavailable or damaged: start fresh.
  }
  return { targets: {}, quality: "balanced" };
}
function savePrefs(update: (p: Prefs) => void) {
  const p = loadPrefs();
  update(p);
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(p));
  } catch {
    // Not remembered; fine.
  }
}

/** Files that can be converted (ffprobe understood them). */
export function usable(files: Probed[]): Probed[] {
  return files.filter((f) => !f.error && f.media.kind);
}

/** Formats every usable file can become, in the first file's order. */
export function commonTargets(files: Probed[]): Target[] {
  const ok = usable(files);
  if (!ok.length) return [];
  return ok[0].targets.filter((t) => ok.every((f) => f.targets.some((o) => o.id === t.id)));
}

function extension(name: string) {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/** The remembered format for this kind, else the first that isn't the file's own. */
function defaultTarget(files: Probed[], targets: Target[]): string | null {
  const ok = usable(files);
  const kind = ok[0]?.media.kind;
  const remembered = kind ? loadPrefs().targets[kind] : undefined;
  if (remembered && targets.some((t) => t.id === remembered)) return remembered;
  const own = new Set(ok.map((f) => extension(f.name).replace("jpeg", "jpg")));
  return (targets.find((t) => !own.has(t.id)) ?? targets[0])?.id ?? null;
}

async function probe(paths: string[]) {
  try {
    const files = await invoke<Probed[]>("convert_probe", { paths });
    staged.set((s) => {
      if (!s || s.paths !== paths) return s;
      const targets = commonTargets(files);
      return { ...s, files, target: defaultTarget(files, targets) };
    });
  } catch {
    // ffmpeg went missing: the island asks to download it again.
  }
}

let started = false;
export function startConvert() {
  if (started) return;
  started = true;
  const apply = (next: Snapshot) => {
    const before = new Map(snapshot.get().jobs.map((j) => [j.id, j.status]));
    const finished = next.jobs
      .filter((j) => (j.status === "done" || j.status === "failed") && before.get(j.id) && before.get(j.id) !== j.status)
      .map((j) => j.id);
    const wasReady = snapshot.get().tools.state === "ready";
    snapshot.set(next);
    if (finished.length) {
      fresh.set((f) => [...f, ...finished]);
      setTimeout(() => fresh.set((f) => f.filter((id) => !finished.includes(id))), FRESH_MS);
    }
    // ffmpeg just arrived: look at the files that were waiting for it.
    const s = staged.get();
    if (!wasReady && next.tools.state === "ready" && s && !s.files) probe(s.paths);
  };
  invoke<Snapshot>("convert_get").then(apply).catch(() => {});
  listen<Snapshot>("convert://changed", (e) => apply(e.payload));
}

export const convertActions = {
  /** Files dropped on the island. */
  stage(paths: string[]) {
    const prefs = loadPrefs();
    const prev = staged.get();
    staged.set({
      paths,
      files: null,
      target: null,
      quality: prefs.quality,
      advanced: prev?.advanced ?? {},
      showAdvanced: false,
    });
    if (snapshot.get().tools.state === "ready") probe(paths);
  },
  unstage: () => staged.set(null),
  setTarget(target: string) {
    staged.set((s) => (s ? { ...s, target } : s));
  },
  setQuality(quality: Quality) {
    staged.set((s) => (s ? { ...s, quality } : s));
    savePrefs((p) => {
      p.quality = quality;
    });
  },
  setAdvanced(patch: Partial<Advanced>) {
    staged.set((s) => (s ? { ...s, advanced: { ...s.advanced, ...patch } } : s));
  },
  resetAdvanced() {
    staged.set((s) => (s ? { ...s, advanced: {} } : s));
  },
  toggleAdvanced() {
    staged.set((s) => (s ? { ...s, showAdvanced: !s.showAdvanced } : s));
  },
  /** Queues the staged files and clears them. */
  async start() {
    const s = staged.get();
    if (!s?.files || !s.target) return;
    const items = usable(s.files).map((f) => ({ path: f.path, target: s.target! }));
    if (!items.length) return;
    const kind = usable(s.files)[0].media.kind;
    if (kind) {
      savePrefs((p) => {
        p.targets[kind] = s.target!;
      });
    }
    staged.set(null);
    await invoke("convert_start", { items, quality: s.quality, advanced: s.advanced });
  },
  install: () => invoke("convert_install"),
  cancelInstall: () => invoke("convert_cancel_install"),
  uninstall: () => invoke("convert_uninstall"),
  cancel: (id: number) => invoke("convert_cancel", { id }),
  clear: () => invoke("convert_clear"),
  reveal: (path: string) => invoke("convert_open", { path, reveal: true }),
  open: (path: string) => invoke("convert_open", { path, reveal: false }),
  dismissFresh: () => fresh.set([]),
};

/** Seconds as "1:05" or "1:02:03". */
export function formatClock(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** "1:23", "83", "1:02:03.5" → seconds; null when it isn't a time. */
export function parseClock(text: string): number | null {
  const t = text.trim().replace(",", ".");
  if (!t) return null;
  if (!/^\d+(:\d{1,2}){0,2}(\.\d+)?$/.test(t)) return null;
  return t.split(":").reduce((acc, part) => acc * 60 + parseFloat(part), 0);
}

/** "24 MB", "830 KB". */
export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

/** Seconds left from progress so far, once there is enough to guess from. */
export function secondsLeft(job: Job, now: number): number | null {
  if (job.status !== "running" || job.startedAt === null || !job.progress || job.progress < 0.03) return null;
  const elapsed = (now - job.startedAt) / 1000;
  return (elapsed / job.progress) * (1 - job.progress);
}
