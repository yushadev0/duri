import { describe, expect, it } from "vitest";
import { formatClock, resolveHour12 } from "./clock";

describe("resolveHour12", () => {
  it("follows Windows unless a format is picked", () => {
    expect(resolveHour12("system", true)).toBe(false);
    expect(resolveHour12("system", false)).toBe(true);
    expect(resolveHour12("12h", true)).toBe(true);
    expect(resolveHour12("24h", false)).toBe(false);
  });
});

describe("formatClock", () => {
  const afternoon = new Date(2026, 10, 3, 15, 4);
  it("uses a 24-hour clock regardless of language", () => {
    expect(formatClock(afternoon, "en", false)).toBe("15:04");
    expect(formatClock(afternoon, "tr", false)).toBe("15:04");
  });
  it("uses a 12-hour clock regardless of language", () => {
    expect(formatClock(afternoon, "en", true)).toMatch(/^3:04\sPM$/);
    expect(formatClock(afternoon, "tr", true)).toMatch(/3:04/);
  });
});
