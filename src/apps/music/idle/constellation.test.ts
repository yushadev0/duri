import { describe, expect, it } from "vitest";
import { crosses, hash, placeLine, random } from "./constellation";

const area = { x0: 150, y0: 170, x1: 1770, y1: 860 };
const sizes = ["Dans", "et,", "benimle", "dans", "et", "tut", "şu", "elimden"].map((w) => ({ w: w.length * 20, h: 40 }));

describe("placeLine", () => {
  it("is the same for the same seed and different for another", () => {
    const a = placeLine(sizes, area, hash("line 1"));
    expect(placeLine(sizes, area, hash("line 1"))).toEqual(a);
    expect(placeLine(sizes, area, hash("line 2"))).not.toEqual(a);
  });

  it("keeps every word inside the area, labels apart and steps comfortable", () => {
    for (let seed = 1; seed < 40; seed++) {
      const pts = placeLine(sizes, area, seed);
      pts.forEach((p, i) => {
        expect(p.x - sizes[i].w / 2).toBeGreaterThanOrEqual(area.x0 - 0.5);
        expect(p.x + sizes[i].w / 2).toBeLessThanOrEqual(area.x1 + 0.5);
        expect(p.y).toBeLessThanOrEqual(area.y1 + 0.5);
        for (let j = 0; j < i; j++) {
          const apart = Math.abs(p.x - pts[j].x) > (sizes[i].w + sizes[j].w) / 2 || Math.abs(p.y - pts[j].y) > 40;
          expect(apart).toBe(true);
        }
        if (i > 0) expect(Math.hypot(p.x - pts[i - 1].x, p.y - pts[i - 1].y)).toBeGreaterThan(120);
      });
    }
  });
});

describe("helpers", () => {
  it("detects crossing segments", () => {
    expect(crosses({ x: 0, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 }, { x: 10, y: 0 })).toBe(true);
    expect(crosses({ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 5 }, { x: 10, y: 5 })).toBe(false);
  });

  it("random is seeded and in [0, 1)", () => {
    const a = random(7);
    const b = random(7);
    const xs = Array.from({ length: 100 }, () => a());
    expect(xs).toEqual(Array.from({ length: 100 }, () => b()));
    expect(xs.every((x) => x >= 0 && x < 1)).toBe(true);
  });
});
