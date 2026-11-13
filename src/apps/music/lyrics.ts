import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createStore, useStore } from "../../lib/store";
import { currentPosition, type MusicState } from "./store";

/** Mirrors `Lyrics` in src-tauri/src/apps/lyrics.rs. */
export interface LyricsRecord {
  found: boolean;
  instrumental: boolean;
  synced: string | null;
  plain: string | null;
  /** Word-synced lyrics (LRCLIB's lyricsfile, read by src-tauri/src/apps/lyricsfile.rs). */
  wordLines?: WordLineRecord[] | null;
}

/** Mirrors `WordLine` in src-tauri/src/apps/lyricsfile.rs. */
export interface WordLineRecord {
  text: string;
  startMs: number;
  endMs: number | null;
  words: { text: string; startMs: number }[];
}

export interface LyricWord {
  /** As sung, punctuation and case kept. */
  text: string;
  /** Lower case without surrounding punctuation: words with the same key are one node. */
  key: string;
  /** Start, in ms of the song. */
  t: number;
  end: number;
  line: number;
  /** Last word of its line. */
  last: boolean;
}

export interface LyricLine {
  t: number;
  end: number;
  text: string;
  /** Index of its first word in `Timeline.words`. */
  firstWord: number;
}

export interface Timeline {
  /** False for plain lyrics: the times are made up and nothing follows the song. */
  synced: boolean;
  /** Every word has its own time (enhanced LRC); otherwise words are spread over their line. */
  wordTimed: boolean;
  lines: LyricLine[];
  words: LyricWord[];
}

const TAG = /\[(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\]/g;
const WORD_TAG = /<(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)>/g;
const OFFSET = /^\[offset:\s*([+-]?\d+)\s*\]/i;

function ms(min: string, sec: string): number {
  return Math.round((Number(min) * 60 + Number(sec.replace(":", "."))) * 1000);
}

/** Lower case, without the punctuation around it ("Love," → "love", "don't" stays). */
export function wordKey(word: string): string {
  return word
    .normalize("NFC")
    .toLocaleLowerCase()
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
}

interface RawLine {
  t: number;
  /** When the line is known to end (word-synced lyrics); otherwise estimated. */
  end?: number;
  text: string;
  /** Word start times (enhanced LRC or a lyricsfile), when present. */
  wordTimes?: number[];
  words: string[];
}

/**
 * Lines from a lyricsfile. Its words carry their trailing space ("Kim "),
 * so pieces without one are syllables of the same word and are joined,
 * keeping the first piece's time.
 */
export function wordLinesToRaw(lines: WordLineRecord[]): RawLine[] {
  return lines
    .map((line) => {
      const words: string[] = [];
      const wordTimes: number[] = [];
      let open = false;
      for (const piece of line.words) {
        const text = piece.text.trim();
        if (text) {
          if (open) words[words.length - 1] += text;
          else {
            words.push(text);
            wordTimes.push(piece.startMs);
          }
        }
        open = !/\s$/.test(piece.text) && text !== "";
      }
      return { t: line.startMs, end: line.endMs ?? undefined, text: line.text.trim(), words, wordTimes };
    })
    .filter((l) => l.words.length)
    .sort((a, b) => a.t - b.t);
}

/** `[mm:ss.xx]` lines, sorted. A line with several tags repeats; `[offset:]` is applied. */
export function parseLrc(text: string): RawLine[] {
  let offset = 0;
  const out: RawLine[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const off = OFFSET.exec(raw.trim());
    if (off) {
      offset = Number(off[1]);
      continue;
    }
    const times: number[] = [];
    let rest = raw;
    TAG.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = TAG.exec(raw)) && m.index === raw.length - rest.length) {
      times.push(ms(m[1], m[2]));
      rest = raw.slice(TAG.lastIndex);
    }
    if (times.length === 0) continue;

    // Enhanced LRC: "<00:12.10>Hello <00:12.60>world".
    const wordTimes: number[] = [];
    const words: string[] = [];
    if (WORD_TAG.test(rest)) {
      for (const part of rest.split(/(?=<\d{1,3}:\d)/)) {
        const wm = /^<(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)>(.*)$/s.exec(part);
        const body = (wm ? wm[3] : part).trim();
        if (!body) continue;
        for (const w of body.split(/\s+/)) {
          words.push(w);
          wordTimes.push(wm ? ms(wm[1], wm[2]) : NaN);
        }
      }
    }
    WORD_TAG.lastIndex = 0;
    const clean = rest.replace(WORD_TAG, "").replace(/\s+/g, " ").trim();
    const plainWords = clean ? clean.split(" ") : [];
    for (const t of times) {
      const usable = words.length === plainWords.length && wordTimes.every((x) => !Number.isNaN(x));
      out.push({
        t: Math.max(0, t - offset),
        text: clean,
        words: plainWords,
        wordTimes: usable && words.length ? wordTimes.map((x) => Math.max(0, x - offset)) : undefined,
      });
    }
  }
  return out.sort((a, b) => a.t - b.t);
}

/**
 * When only a line's start is known, its words are spread over the time
 * until the next line, up to what a slow singer would need (so a short line
 * before a long instrumental break doesn't stretch across the break).
 */
const SLOW_MS_PER_CHAR = 190;
const MIN_LINE_MS = 900;
/** Share of the line's time the words take; the rest is the breath before the next line. */
const SUNG_SHARE = 0.86;

/**
 * Spreads each line's words over the time it is sung, weighted by length
 * (LRCLIB lyrics are timed per line). A line ends when the next one starts,
 * or sooner if it is short and a long instrumental gap follows.
 */
export function buildTimeline(raw: RawLine[], durationMs: number): Timeline {
  const lines: LyricLine[] = [];
  const words: LyricWord[] = [];
  raw.forEach((r, i) => {
    const next = raw[i + 1]?.t ?? Math.max(durationMs, r.t + 4000);
    const chars = r.words.reduce((n, w) => n + w.length, 0);
    const end = r.end ?? Math.min(next, r.t + Math.max(MIN_LINE_MS, chars * SLOW_MS_PER_CHAR + 600));
    const lineIndex = lines.length;
    lines.push({ t: r.t, end, text: r.text, firstWord: words.length });
    const sung = r.words.map((w) => ({ w, key: wordKey(w) }));
    const total = sung.reduce((n, s) => n + s.w.length + 2, 0);
    let acc = 0;
    sung.forEach((s, j) => {
      const t = r.wordTimes ? r.wordTimes[j] : r.t + (acc / total) * (end - r.t) * SUNG_SHARE;
      acc += s.w.length + 2;
      if (!s.key) return;
      words.push({ text: s.w, key: s.key, t, end, line: lineIndex, last: false });
    });
  });
  // Each word lasts until the next one in its line; mark line endings.
  for (let i = 0; i < words.length; i++) {
    const w = words[i];
    const n = words[i + 1];
    if (!n || n.line !== w.line) w.last = true;
    else w.end = n.t;
  }
  const sung = raw.filter((r) => r.words.length);
  const wordTimed = sung.length > 0 && sung.every((r) => r.wordTimes);
  return { synced: true, wordTimed, lines, words };
}

/** Plain lyrics: lines without times, every word "already sung". */
export function plainTimeline(text: string): Timeline {
  const raw = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l, i, all) => l || (i > 0 && all[i - 1]))
    .map((text) => ({ t: 0, text, words: text ? text.split(/\s+/) : [] }));
  const tl = buildTimeline(raw, 0);
  return { ...tl, synced: false, wordTimed: false };
}

export function timelineFrom(record: LyricsRecord, durationMs: number): Timeline | null {
  if (record.wordLines?.length) {
    const raw = wordLinesToRaw(record.wordLines);
    if (raw.length) return buildTimeline(raw, durationMs);
  }
  if (record.synced) {
    const raw = parseLrc(record.synced);
    if (raw.length) return buildTimeline(raw, durationMs);
  }
  if (record.plain) return plainTimeline(record.plain);
  return null;
}

/** Index of the last item with `t <= at`, or -1. */
export function indexAt(items: { t: number }[], at: number): number {
  let lo = 0;
  let hi = items.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (items[mid].t <= at) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

export type LyricsStatus =
  | { kind: "loading" }
  | { kind: "ready"; timeline: Timeline }
  | { kind: "instrumental" }
  | { kind: "none" }
  | { kind: "error" }
  | { kind: "idle" };

/**
 * Lyrics per track, shared by everything in the window that shows them (the
 * pill, the expanded view, Idle Mode), so a song is asked for only once.
 * Keyed by track and by whether its length was known, since the length is
 * part of the lookup and often arrives a moment after the title.
 */
const lyricsStore = createStore<ReadonlyMap<string, LyricsStatus>>(new Map());
const requested = new Set<string>();
const KEEP = 24;

function request(key: string, query: { artist: string; title: string; album: string; durationMs: number }) {
  if (requested.has(key)) return;
  requested.add(key);
  const settle = (status: LyricsStatus) =>
    lyricsStore.set((prev) => {
      const next = new Map(prev);
      next.set(key, status);
      // Forget the oldest songs; they are in the backend's cache anyway.
      for (const old of next.keys()) {
        if (next.size <= KEEP) break;
        next.delete(old);
        requested.delete(old);
      }
      return next;
    });
  invoke<LyricsRecord>("lyrics_get", query)
    .then((rec) => {
      const timeline = rec.found ? timelineFrom(rec, query.durationMs) : null;
      settle(timeline ? { kind: "ready", timeline } : rec.instrumental ? { kind: "instrumental" } : { kind: "none" });
    })
    .catch(() => {
      // A network error may pass: let the next view of this song ask again.
      requested.delete(key);
      settle({ kind: "error" });
    });
}

/**
 * Lyrics for the current song, and nothing at all while `enabled` is false.
 * A short delay rides out the title flickering while a player switches.
 */
export function useLyrics(state: MusicState, enabled: boolean): LyricsStatus {
  const { trackKey, title, artist, album, durationMs, available } = state;
  // Rounded, so small corrections of the length don't ask again.
  const duration = Math.round(durationMs / 1000);
  const wanted = enabled && available && title ? `${trackKey}|${duration > 0 ? "timed" : "untimed"}` : "";
  const statuses = useStore(lyricsStore);

  useEffect(() => {
    if (!wanted) return;
    const id = setTimeout(() => request(wanted, { artist, title, album, durationMs: duration * 1000 }), 350);
    return () => clearTimeout(id);
    // The key covers the track and whether the length is known.
  }, [wanted]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!wanted) return { kind: "idle" };
  return statuses.get(wanted) ?? { kind: "loading" };
}

/**
 * Index of the line being sung, re-rendering only when it changes (a timer
 * set for the next line's start, not every frame).
 */
export function useLineIndex(lines: LyricLine[], state: MusicState, synced: boolean): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!synced) return;
    const t = Date.now();
    const pos = currentPosition(state, t);
    const live = indexAt(lines, pos);
    let delay: number | null = null;
    if (live !== indexAt(lines, currentPosition(state, now))) delay = 0;
    else if (state.playing && lines[live + 1]) delay = lines[live + 1].t - pos + 10;
    if (delay === null) return;
    const id = setTimeout(() => setNow(Date.now()), Math.max(0, delay));
    return () => clearTimeout(id);
  }, [lines, state, now, synced]);
  return synced ? indexAt(lines, currentPosition(state, now)) : -1;
}
