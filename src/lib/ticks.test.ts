import { describe, expect, it } from "vitest";
import { nextBoundary } from "./ticks";

describe("nextBoundary", () => {
  it("finds the next minute", () => {
    expect(nextBoundary(125_000, 60_000)).toBe(180_000);
    expect(nextBoundary(120_000, 60_000)).toBe(180_000);
  });

  it("follows a phase, e.g. a countdown ending at 10_400", () => {
    expect(nextBoundary(7_000, 1000, 10_400)).toBe(7_400);
    expect(nextBoundary(7_400, 1000, 10_400)).toBe(8_400);
  });

  it("handles a phase in the future or past alike", () => {
    expect(nextBoundary(5_000, 1000, 2_250)).toBe(5_250);
    expect(nextBoundary(5_000, 1000, 9_250)).toBe(5_250);
  });
});
