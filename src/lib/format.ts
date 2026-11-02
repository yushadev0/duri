const pad = (n: number) => String(n).padStart(2, "0");

/** Countdown text, rounded up so a timer never shows 0:00 before it ends. */
export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** Stopwatch text: `mm:ss` or `mm:ss.cc` (hours added when needed). */
export function formatStopwatch(ms: number, precise = true, decimal = "."): string {
  const t = Math.max(0, Math.floor(ms));
  const h = Math.floor(t / 3_600_000);
  const m = Math.floor(t / 60_000) % 60;
  const s = Math.floor(t / 1000) % 60;
  const cs = Math.floor((t % 1000) / 10);
  const base = h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
  return precise ? `${base}${decimal}${pad(cs)}` : base;
}

/** Track position text: `m:ss` or `h:mm:ss`. */
export function formatTrackTime(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** Short duration for timer presets and labels: "5 min", "1 h 30 min", "45 s". */
export function formatDurationShort(ms: number, units: { h: string; min: string; s: string }): string {
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const parts: string[] = [];
  if (h) parts.push(`${h} ${units.h}`);
  if (m) parts.push(`${m} ${units.min}`);
  if (s || parts.length === 0) parts.push(`${s} ${units.s}`);
  return parts.join(" ");
}
