import { describe, expect, it } from "vitest";
import { initialIslandState as init, islandReducer as r, type IslandState } from "./state";

const run = (actions: Parameters<typeof r>[1][], from: IslandState = init) => actions.reduce(r, from);

describe("islandReducer", () => {
  it("hover opens compact only while the pointer is inside", () => {
    expect(run([{ type: "hover" }]).mode).toBe("idle");
    const s = run([{ type: "pointer", inside: true }, { type: "hover" }]);
    expect(s.mode).toBe("compact");
    expect(s.growing).toBe(true);
  });

  it("leave collapses unless the pointer came back", () => {
    const open = run([{ type: "pointer", inside: true }, { type: "hover" }]);
    expect(run([{ type: "leave" }], open).mode).toBe("compact");
    const left = run([{ type: "pointer", inside: false }, { type: "leave" }], open);
    expect(left.mode).toBe("idle");
    expect(left.growing).toBe(false);
  });

  it("open expands an app and leaving collapses it", () => {
    const s = run([{ type: "pointer", inside: true }, { type: "open", appId: "music" }]);
    expect(s).toMatchObject({ mode: "expanded", appId: "music" });
    expect(run([{ type: "pointer", inside: false }, { type: "leave" }], s).mode).toBe("idle");
  });

  it("attention holds the island in compact until it ends", () => {
    const s = run([{ type: "attention", appId: "timer" }]);
    expect(s).toMatchObject({ mode: "compact", attention: "timer" });
    expect(run([{ type: "leave" }], s).mode).toBe("compact");
    expect(run([{ type: "collapse" }], s).mode).toBe("compact");
    const ended = run([{ type: "attentionEnd" }], s);
    expect(ended).toMatchObject({ mode: "idle", attention: null });
  });

  it("attention while expanded switches to that app without collapsing", () => {
    const s = run([{ type: "open", appId: "music" }, { type: "attention", appId: "timer" }]);
    expect(s).toMatchObject({ mode: "expanded", appId: "timer", attention: "timer" });
    // Collapsing then rests on the alert.
    expect(run([{ type: "collapse" }], s).mode).toBe("compact");
  });

  it("attention end keeps compact while hovered", () => {
    const s = run([{ type: "attention", appId: "timer" }, { type: "pointer", inside: true }, { type: "attentionEnd" }]);
    expect(s.mode).toBe("compact");
  });
});
