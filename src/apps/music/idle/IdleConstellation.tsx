import { useEffect, useRef } from "react";
import { indexAt, type Timeline } from "../lyrics";
import { currentPosition, getMusic, subscribeMusic } from "../store";
import { hash, LABEL_GAP, placeLine, random, type Point } from "./constellation";
import styles from "./idle.module.css";
import { follow, getLevel, type Envelope } from "./meter";

/** The line to a word starts this long before the word, so the word lands on the beat. */
const LEAD_MS = 300;
/** A word's label fades in over this long. */
const APPEAR_MS = 260;
/** The ripple around a word that just landed. */
const RIPPLE_MS = 1000;
/** A finished line fades out over this long, while the next one begins. */
const FADE_MS = 900;
/** A line with nothing after it stays this long past its last word. */
const LINGER_MS = 2500;
/** The circle steps aside this long before the next line starts. */
const CIRCLE_EXIT_MS = 700;
const CIRCLE_FADE_MS = 450;
const HISTORY = 96;
const STARS = 150;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const easeOut = (t: number) => 1 - (1 - t) * (1 - t);

interface Star {
  x: number;
  y: number;
  r: number;
  alpha: number;
  period: number;
  phase: number;
}

/**
 * Network: every line of the song as a constellation. Words land at random
 * spots one by one, each joined to the one before; at the end of the line
 * the figure drifts up and fades, and the next line starts on an empty sky
 * (see `constellation.ts`). Where nothing is sung (intro, breaks, outro, or
 * an instrumental) a circle breathes with the sound instead.
 *
 * Everything moves slowly and softly: stars twinkle over seconds, glows rise
 * and fade, the circle follows a VU-like envelope. Under reduced motion only
 * fades remain. The loop runs while the song plays or something is still
 * fading, and stops at rest.
 */
export function IdleConstellation({
  timeline,
  accent,
  reduceMotion,
}: {
  /** Null for an instrumental: only the circle. */
  timeline: Timeline | null;
  accent: string;
  reduceMotion: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const accentRef = useRef(accent);
  const reduceRef = useRef(reduceMotion);
  const kickRef = useRef<() => void>(() => {});

  useEffect(() => {
    accentRef.current = accent;
    reduceRef.current = reduceMotion;
    kickRef.current();
  }, [accent, reduceMotion]);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext("2d")!;
    const lines = timeline?.lines ?? [];
    const words = timeline?.words ?? [];
    let width = 0;
    let height = 0;
    let dpr = 1;
    let fontSize = 36;
    let font = "";
    let raf = 0;
    let wakeUntil = 0;
    let lastFrame = 0;
    let stars: Star[] = [];
    /** Placed lines, by index; cleared when the window size changes. */
    const layouts = new Map<number, Point[]>();
    let env: Envelope = { peak: 0.04, value: 0 };
    const history = new Float32Array(HISTORY);
    let head = 0;
    let circleAlpha = 0;
    let spin = 0;

    const wordsOf = (li: number) => {
      const out: number[] = [];
      for (let k = lines[li].firstWord; k < words.length && words[k].line === li; k++) out.push(k);
      return out;
    };

    const layout = (li: number): Point[] => {
      let pts = layouts.get(li);
      if (!pts) {
        ctx.font = font;
        const sizes = wordsOf(li).map((k) => ({ w: ctx.measureText(words[k].text).width, h: fontSize }));
        // Leave the top for the controls and the bottom corner for the song.
        const area = { x0: width * 0.07, x1: width * 0.93, y0: height * 0.13, y1: height * 0.8 };
        pts = placeLine(sizes, area, hash(`${li}\u0000${lines[li].text}`));
        layouts.set(li, pts);
      }
      return pts;
    };

    /** 1 while the line is on, then fading to 0 once it is over. */
    const lineAlpha = (li: number, pos: number) => {
      const line = lines[li];
      const next = lines[li + 1];
      if (!wordsOf(li).length || pos < line.t - LEAD_MS) return 0;
      const over = Math.min(next ? next.t : Infinity, line.end + LINGER_MS);
      return pos < over ? 1 : clamp(1 - (pos - over) / FADE_MS, 0, 1);
    };

    const drawStars = (now: number, reduce: boolean) => {
      ctx.fillStyle = "#fff";
      for (const s of stars) {
        const twinkle = reduce ? 1 : 0.55 + 0.45 * Math.sin(now / s.period + s.phase);
        ctx.globalAlpha = s.alpha * twinkle;
        ctx.beginPath();
        ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
        ctx.fill();
      }
    };

    const drawLine = (li: number, alpha: number, pos: number, color: string, reduce: boolean) => {
      const ks = wordsOf(li);
      const pts = layout(li);
      const lead = reduce ? 0 : LEAD_MS;
      let latest = -1;
      ks.forEach((k, j) => {
        if (words[k].t <= pos) latest = j;
      });

      ctx.save();
      // A finished figure drifts up as it fades.
      if (!reduce) ctx.translate(0, -(1 - alpha) * 26);

      // Lines first, so labels sit on top of them.
      ctx.lineCap = "round";
      ks.forEach((k, j) => {
        if (j === 0) return;
        const start = words[k].t - lead;
        if (pos < start) return;
        const p = lead ? easeOut(clamp((pos - start) / lead, 0, 1)) : 1;
        const a = pts[j - 1];
        const b = pts[j];
        const tip = { x: a.x + (b.x - a.x) * p, y: a.y + (b.y - a.y) * p };
        const live = j === latest || p < 1;
        ctx.globalAlpha = alpha * (live ? 0.9 : 0.46);
        ctx.strokeStyle = color;
        ctx.lineWidth = live ? 2.4 : 1.8;
        ctx.shadowColor = color;
        ctx.shadowBlur = live ? 12 : 0;
        ctx.beginPath();
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(tip.x, tip.y);
        ctx.stroke();
        // A spark running ahead of the line.
        if (p < 1) {
          ctx.globalAlpha = alpha;
          ctx.fillStyle = "#fff";
          ctx.shadowBlur = 16;
          ctx.beginPath();
          ctx.arc(tip.x, tip.y, 3, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.shadowBlur = 0;
      });

      ctx.font = font;
      ctx.textAlign = "center";
      ctx.textBaseline = "alphabetic";
      ks.forEach((k, j) => {
        const w = words[k];
        if (pos < w.t) return;
        const age = pos - w.t;
        const k01 = clamp(age / APPEAR_MS, 0, 1);
        const shown = alpha * easeOut(k01);
        const current = j === latest && alpha === 1;
        const pt = pts[j];

        // A ring spreading out where the word landed.
        if (!reduce && age < RIPPLE_MS) {
          const e = easeOut(age / RIPPLE_MS);
          ctx.globalAlpha = alpha * 0.5 * (1 - e);
          ctx.strokeStyle = color;
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.arc(pt.x, pt.y, 5 + e * 34, 0, Math.PI * 2);
          ctx.stroke();
        }

        // The star.
        ctx.globalAlpha = shown;
        ctx.fillStyle = current ? color : "#fff";
        ctx.shadowColor = color;
        ctx.shadowBlur = current ? 14 : 0;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, current ? 4.5 : 3.5, 0, Math.PI * 2);
        ctx.fill();

        // The word: lands with a little lift and settle, glows while sung.
        const settle = 1 - easeOut(k01);
        const rise = reduce ? 0 : settle * 10;
        const scale = reduce ? 1 : 1 + settle * 0.12;
        ctx.globalAlpha = shown * (current ? 1 : 0.62);
        ctx.fillStyle = "#fff";
        ctx.shadowBlur = current ? 22 : 0;
        ctx.save();
        ctx.translate(pt.x, pt.y - LABEL_GAP + rise);
        ctx.scale(scale, scale);
        ctx.fillText(w.text, 0, 0);
        ctx.restore();
        ctx.shadowBlur = 0;
      });
      ctx.restore();
    };

    /** The sound-reactive circle: a ring drawn from the last ~1.5 s of levels. */
    const drawCircle = (alpha: number, color: string, reduce: boolean) => {
      const cx = width / 2;
      const cy = height * 0.465;
      const radius = Math.min(width, height) * 0.13;
      const level = env.value;

      // Soft light in the middle.
      const glow = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius * 2.4);
      glow.addColorStop(0, color);
      glow.addColorStop(1, "rgba(0, 0, 0, 0)");
      ctx.globalAlpha = alpha * (0.1 + level * 0.14);
      ctx.fillStyle = glow;
      ctx.fillRect(cx - radius * 2.4, cy - radius * 2.4, radius * 4.8, radius * 4.8);

      // Inner ring, barely moving.
      ctx.globalAlpha = alpha * 0.22;
      ctx.strokeStyle = "#fff";
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(cx, cy, radius * (0.78 + level * 0.03), 0, Math.PI * 2);
      ctx.stroke();

      // Outer ring: the newest level at the top, older ones around both sides.
      const n = 180;
      ctx.beginPath();
      for (let i = 0; i <= n; i++) {
        const u = Math.abs((i / n) * 2 - 1);
        const age = Math.round((1 - u) * (HISTORY - 1));
        const h = reduce ? level : history[(head - 1 - age + HISTORY * 2) % HISTORY];
        const r = radius * (1 + level * 0.05) + h * radius * 0.26;
        const angle = (i / n) * Math.PI * 2 - Math.PI / 2 + spin;
        const x = cx + Math.cos(angle) * r;
        const y = cy + Math.sin(angle) * r;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();
      ctx.globalAlpha = alpha * 0.9;
      ctx.strokeStyle = color;
      ctx.lineWidth = 2.2;
      ctx.shadowColor = color;
      ctx.shadowBlur = 18;
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Three small moons circling it, a little faster when it's loud.
      if (!reduce) {
        ctx.fillStyle = "#fff";
        for (let m = 0; m < 3; m++) {
          const a = spin * (1.6 + m * 0.35) + (m * Math.PI * 2) / 3;
          const r = radius * (1.42 + m * 0.12);
          ctx.globalAlpha = alpha * (0.35 + level * 0.3);
          ctx.beginPath();
          ctx.arc(cx + Math.cos(a) * r, cy + Math.sin(a) * r, 2.2, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    };

    const frame = () => {
      raf = 0;
      const now = Date.now();
      const dt = lastFrame ? Math.min(100, now - lastFrame) : 16;
      lastFrame = now;
      const { state } = getMusic();
      const pos = currentPosition(state, now);
      const color = accentRef.current;
      const reduce = reduceRef.current;

      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawStars(now, reduce);

      // The line being sung (or about to be: its first stroke leads it), and the one fading out.
      const li = indexAt(lines, pos + LEAD_MS);
      let busy = false;
      let sung = false;
      for (const i of [li - 1, li]) {
        if (i < 0) continue;
        const a = lineAlpha(i, pos);
        if (a <= 0) continue;
        sung = true;
        if (a < 1) busy = true;
        drawLine(i, a, pos, color, reduce);
      }

      // The circle fills the quiet parts and steps aside just before words come.
      const ni = indexAt(lines, pos) + 1;
      const wordsSoon = ni < lines.length && wordsOf(ni).length > 0 && lines[ni].t - pos < CIRCLE_EXIT_MS;
      const target = !sung && !wordsSoon ? 1 : 0;
      circleAlpha += (target - circleAlpha) * Math.min(1, dt / CIRCLE_FADE_MS);
      env = follow(env, state.playing ? getLevel() : 0, dt);
      history[head] = env.value;
      head = (head + 1) % HISTORY;
      if (!reduce && state.playing) spin += dt * (0.00012 + env.value * 0.00025);
      if (circleAlpha > 0.01) drawCircle(circleAlpha, color, reduce);
      ctx.globalAlpha = 1;

      const settling = Math.abs(target - circleAlpha) > 0.01 || env.value > 0.01;
      if (state.playing || busy || settling || now < wakeUntil) raf = requestAnimationFrame(frame);
      else lastFrame = 0;
    };

    function kick() {
      wakeUntil = Date.now() + 600;
      if (!raf) raf = requestAnimationFrame(frame);
    }
    kickRef.current = kick;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      dpr = window.devicePixelRatio || 1;
      width = rect.width;
      height = rect.height;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      fontSize = Math.round(clamp(width * 0.024, 26, 48));
      font = `600 ${fontSize}px "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`;
      layouts.clear();
      const rnd = random(42);
      stars = Array.from({ length: STARS }, () => ({
        x: rnd() * width,
        y: rnd() * height,
        r: 0.5 + rnd() * 1.1,
        alpha: 0.08 + rnd() * 0.3,
        // sin(now / period): one twinkle every ~3 – 7.5 s.
        period: 500 + rnd() * 700,
        phase: rnd() * Math.PI * 2,
      }));
      kick();
    };

    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    const unsubscribe = subscribeMusic(kick);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      unsubscribe();
      kickRef.current = () => {};
    };
  }, [timeline]);

  return <canvas ref={canvasRef} className={styles.canvas} aria-hidden />;
}
