import { useEffect, useState } from "react";

/**
 * Next moment after `now` that lies on the grid `phase + k * period`.
 * Used to re-render exactly when a displayed value changes (a new minute,
 * the next second of a countdown) instead of polling.
 */
export function nextBoundary(now: number, period: number, phase = 0): number {
  const offset = (((now - phase) % period) + period) % period;
  return now + (period - offset);
}

/**
 * Current time, updated only on the `period` grid shifted by `phase`
 * (e.g. every minute for a clock, every second of a countdown that ends at
 * `phase`). While disabled the last value is kept.
 */
export function useAlignedNow(enabled: boolean, period: number, phase = 0): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!enabled) return;
    let id: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(Date.now());
      const t = Date.now();
      // A few ms past the boundary so the new value has definitely arrived.
      id = setTimeout(tick, nextBoundary(t, period, phase) - t + 5);
    };
    // Catch up right away (the value may be stale after being disabled).
    id = setTimeout(tick, 0);
    return () => clearTimeout(id);
  }, [enabled, period, phase]);
  return now;
}

/**
 * A counter that advances every `intervalMs` while enabled. Drives stepped
 * animations in the always-visible pill: a few repaints a second instead of
 * a CSS animation repainting the whole transparent window at the display's
 * refresh rate.
 */
export function useStep(enabled: boolean, intervalMs: number): number {
  const [step, setStep] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    const id = setInterval(() => setStep((s) => s + 1), intervalMs);
    return () => clearInterval(id);
  }, [enabled, intervalMs]);
  return step;
}
