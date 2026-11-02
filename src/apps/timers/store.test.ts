import { describe, expect, it } from "vitest";
import { primaryTimer, remainingMs, stopwatchElapsed, type Timer } from "./store";

const base = { label: null, durationMs: 60_000, createdAt: 0 };
const running = (id: string, endsAt: number): Timer => ({ ...base, id, state: "running", endsAt });

describe("timers", () => {
  it("computes remaining time per state", () => {
    expect(remainingMs(running("a", 10_000), 4_000)).toBe(6_000);
    expect(remainingMs(running("a", 10_000), 40_000)).toBe(0);
    expect(remainingMs({ ...base, id: "b", state: "paused", remainingMs: 7 }, 99)).toBe(7);
    expect(remainingMs({ ...base, id: "c", state: "finished", finishedAt: 1 }, 99)).toBe(0);
  });

  it("prefers finished, then the soonest running timer", () => {
    const paused: Timer = { ...base, id: "p", state: "paused", remainingMs: 1 };
    expect(primaryTimer([paused, running("late", 50), running("soon", 10)])?.id).toBe("soon");
    const done: Timer = { ...base, id: "d", state: "finished", finishedAt: 1 };
    expect(primaryTimer([running("soon", 10), done])?.id).toBe("d");
    expect(primaryTimer([paused])?.id).toBe("p");
    expect(primaryTimer([])).toBeUndefined();
  });
});

describe("stopwatch", () => {
  it("adds the running segment to banked time", () => {
    expect(stopwatchElapsed({ runningSince: 1_000, accumulatedMs: 500, laps: [] }, 3_000)).toBe(2_500);
    expect(stopwatchElapsed({ runningSince: null, accumulatedMs: 500, laps: [] }, 3_000)).toBe(500);
  });
});
