import { useEffect, useState } from "react";

/**
 * Current time that re-renders while `enabled`. Without `intervalMs` it
 * updates every animation frame (for smooth digits); otherwise on an interval.
 * While disabled the last value is kept, which is fine for every caller:
 * paused things don't depend on the clock.
 */
export function useNow(enabled: boolean, intervalMs?: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!enabled) return;
    const tick = () => setNow(Date.now());
    let raf = requestAnimationFrame(function loop() {
      tick();
      if (!intervalMs) raf = requestAnimationFrame(loop);
    });
    const id = intervalMs ? setInterval(tick, intervalMs) : undefined;
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(id);
    };
  }, [enabled, intervalMs]);
  return now;
}
