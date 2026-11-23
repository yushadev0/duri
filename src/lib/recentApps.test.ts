import { describe, expect, it } from "vitest";
import { pickRecent } from "./recentApps";

const apps = ["music", "sessions", "notes", "convert", "timer"].map((id) => ({ id }));
const ids = (l: { id: string }[]) => l.map((a) => a.id);

describe("recent apps", () => {
  it("offers the most recently opened first", () => {
    expect(ids(pickRecent(apps, ["convert", "timer", "music", "notes"], 3))).toEqual(["convert", "timer", "music"]);
  });

  it("fills up with the dock order", () => {
    expect(ids(pickRecent(apps, ["timer"], 3))).toEqual(["timer", "music", "sessions"]);
    expect(ids(pickRecent(apps, [], 3))).toEqual(["music", "sessions", "notes"]);
  });

  it("skips apps that are off or gone", () => {
    expect(ids(pickRecent(apps, ["clipboard", "notes"], 3))).toEqual(["notes", "music", "sessions"]);
  });
});
