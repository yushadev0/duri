import { describe, expect, it } from "vitest";
import { formatCountdown, formatDurationShort, formatStopwatch, formatTrackTime } from "./format";

describe("formatCountdown", () => {
  it("rounds up partial seconds", () => {
    expect(formatCountdown(59_001)).toBe("1:00");
    expect(formatCountdown(1)).toBe("0:01");
    expect(formatCountdown(0)).toBe("0:00");
    expect(formatCountdown(-5)).toBe("0:00");
  });
  it("adds hours", () => {
    expect(formatCountdown(3_600_000 + 61_000)).toBe("1:01:01");
  });
});

describe("formatStopwatch", () => {
  it("formats centiseconds", () => {
    expect(formatStopwatch(83_456)).toBe("01:23.45");
    expect(formatStopwatch(83_456, true, ",")).toBe("01:23,45");
    expect(formatStopwatch(83_456, false)).toBe("01:23");
  });
  it("adds hours", () => {
    expect(formatStopwatch(3_600_000 * 2 + 5_000, false)).toBe("2:00:05");
  });
});

describe("formatTrackTime", () => {
  it("formats minutes and hours", () => {
    expect(formatTrackTime(65_999)).toBe("1:05");
    expect(formatTrackTime(3_725_000)).toBe("1:02:05");
  });
});

describe("formatDurationShort", () => {
  const units = { h: "h", min: "min", s: "s" };
  it("joins non-zero parts", () => {
    expect(formatDurationShort(300_000, units)).toBe("5 min");
    expect(formatDurationShort(5_400_000, units)).toBe("1 h 30 min");
    expect(formatDurationShort(45_000, units)).toBe("45 s");
    expect(formatDurationShort(0, units)).toBe("0 s");
  });
});
