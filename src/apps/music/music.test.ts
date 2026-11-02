import { describe, expect, it } from "vitest";
import { islandAccent, NEUTRAL_ACCENT, rgbToHsl } from "./accent";
import { sourceName } from "./sources";
import { currentPosition, type MusicState } from "./store";

describe("accent", () => {
  it("converts rgb to hsl", () => {
    const [h, s, l] = rgbToHsl(255, 0, 0);
    expect([Math.round(h), s, l]).toEqual([0, 1, 0.5]);
  });
  it("lifts dark colors and keeps hue", () => {
    expect(islandAccent(80, 0, 0)).toBe("hsl(0 100% 58%)");
  });
  it("uses a neutral accent for grey covers", () => {
    expect(islandAccent(120, 120, 125)).toBe(NEUTRAL_ACCENT);
  });
});

describe("sourceName", () => {
  it("maps known players and falls back to the executable name", () => {
    expect(sourceName("Spotify.exe")).toBe("Spotify");
    expect(sourceName("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic")).toBe("Media Player");
    expect(sourceName("C:\\Apps\\Strawberry.exe")).toBe("Strawberry");
    expect(sourceName("")).toBe("");
  });
});

describe("currentPosition", () => {
  const base = { playing: true, positionMs: 10_000, durationMs: 60_000, positionUpdatedAt: 1_000 } as MusicState;
  it("extrapolates while playing and clamps to the duration", () => {
    expect(currentPosition(base, 3_000)).toBe(12_000);
    expect(currentPosition(base, 999_999)).toBe(60_000);
  });
  it("stays put while paused", () => {
    expect(currentPosition({ ...base, playing: false }, 9_000)).toBe(10_000);
  });
});
