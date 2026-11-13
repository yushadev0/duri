/**
 * Layout for the Network look: each line of the song becomes a small
 * constellation. Every word lands at a random spot, a line reaches it from
 * the word before, and when the line is over the whole figure fades so the
 * next one starts on an empty sky.
 *
 * Positions are random but seeded by the line, so seeking or reopening
 * draws the same figure.
 */

export interface Size {
  w: number;
  h: number;
}

export interface Area {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Where a word's star sits; its label is drawn centered above it. */
export interface Point {
  x: number;
  y: number;
}

/** Gap kept between a label and the star under it. */
export const LABEL_GAP = 14;
const PADDING = 22;
const TRIES = 60;

/** Small, fast, seedable PRNG (mulberry32). */
export function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return h >>> 0;
}

interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** The label box plus its star, with room around it. */
function boxOf(p: Point, s: Size): Rect {
  return {
    x0: p.x - s.w / 2 - PADDING,
    x1: p.x + s.w / 2 + PADDING,
    y0: p.y - LABEL_GAP - s.h - PADDING,
    y1: p.y + PADDING / 2,
  };
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
}

/** Whether segment ab crosses segment cd (shared ends don't count). */
export function crosses(a: Point, b: Point, c: Point, d: Point): boolean {
  const cross = (p: Point, q: Point, r: Point) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const d1 = cross(c, d, a);
  const d2 = cross(c, d, b);
  const d3 = cross(a, b, c);
  const d4 = cross(a, b, d);
  return d1 * d2 < 0 && d3 * d4 < 0;
}

/** Does segment ab pass through the rectangle (roughly: samples along it)? */
function hitsBox(a: Point, b: Point, r: Rect): boolean {
  for (let t = 0.15; t <= 0.85; t += 0.1) {
    const x = a.x + (b.x - a.x) * t;
    const y = a.y + (b.y - a.y) * t;
    if (x > r.x0 && x < r.x1 && y > r.y0 && y < r.y1) return true;
  }
  return false;
}

/**
 * Places the words of one line. Each word tries a few dozen random spots
 * and keeps the best: no overlapping labels, a comfortable step from the
 * previous word (not on top of it, not across the screen), and no line
 * crossing an earlier line or label.
 */
export function placeLine(sizes: Size[], area: Area, seed: number): Point[] {
  const rnd = random(seed);
  const width = area.x1 - area.x0;
  const near = width * 0.14;
  const far = width * 0.36;
  const placed: Point[] = [];
  const boxes: Rect[] = [];

  for (const s of sizes) {
    const prev = placed[placed.length - 1];
    let best: Point | null = null;
    let bestScore = Infinity;
    for (let i = 0; i < TRIES; i++) {
      const p = {
        x: area.x0 + s.w / 2 + rnd() * Math.max(1, width - s.w),
        y: area.y0 + s.h + LABEL_GAP + rnd() * Math.max(1, area.y1 - area.y0 - s.h - LABEL_GAP),
      };
      const box = boxOf(p, s);
      let score = 0;
      for (const b of boxes) if (overlaps(box, b)) score += 1000;
      if (prev) {
        const d = Math.hypot(p.x - prev.x, p.y - prev.y);
        if (d < near) score += (near - d) * 2;
        if (d > far) score += d - far;
        for (let j = 1; j < placed.length - 1; j++) if (crosses(prev, p, placed[j - 1], placed[j])) score += 300;
        for (let j = 0; j < boxes.length - 1; j++) if (hitsBox(prev, p, boxes[j])) score += 200;
      }
      if (score < bestScore) {
        bestScore = score;
        best = p;
        if (score === 0) break;
      }
    }
    placed.push(best!);
    boxes.push(boxOf(best!, s));
  }
  return placed;
}
