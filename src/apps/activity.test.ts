import { describe, expect, it } from "vitest";
import { rankActivities, sameActivity } from "./activity";
import type { DuriApp } from "./types";

const app = (id: string) => ({ id }) as DuriApp;

describe("rankActivities", () => {
  it("sorts by priority and drops apps without activity", () => {
    const apps = [app("music"), app("timer"), app("stopwatch")];
    const ranked = rankActivities(apps, {
      music: { priority: 40 },
      timer: { priority: 60 },
      stopwatch: null,
    });
    expect(ranked.map((r) => r.app.id)).toEqual(["timer", "music"]);
  });

  it("keeps registry order on ties", () => {
    const ranked = rankActivities([app("a"), app("b")], { b: { priority: 1 }, a: { priority: 1 } });
    expect(ranked.map((r) => r.app.id)).toEqual(["a", "b"]);
  });
});

describe("sameActivity", () => {
  it("compares the fields that matter", () => {
    expect(sameActivity(null, undefined)).toBe(true);
    expect(sameActivity({ priority: 1 }, null)).toBe(false);
    expect(sameActivity({ priority: 1 }, { priority: 1, alert: false })).toBe(true);
    expect(sameActivity({ priority: 1, key: "a" }, { priority: 1, key: "b" })).toBe(false);
  });
});
