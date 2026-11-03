import { describe, expect, it } from "vitest";
import { focusSession, keepSelection, type Session } from "./store";

const s = (id: string, status: Session["status"], statusSince = 1): Session => ({
  id,
  agent: "claude",
  project: id,
  cwd: "",
  status,
  detail: null,
  prompt: null,
  startedAt: 0,
  updatedAt: 0,
  statusSince,
  permission: null,
});

describe("focusSession", () => {
  it("keeps showing the same session while others reorder", () => {
    expect(focusSession([s("a", "working"), s("b", "done")], [])?.id).toBe("a");
    // The backend re-ranks: b is now first, but a is still there.
    expect(focusSession([s("b", "working"), s("a", "done")], [])?.id).toBe("a");
  });

  it("switches to a session that needs you, unless already acknowledged", () => {
    focusSession([s("a", "working")], []);
    expect(focusSession([s("b", "waiting", 5), s("a", "working")], [])?.id).toBe("b");
    expect(focusSession([s("b", "waiting", 5), s("a", "working")], ["b:5"])?.id).toBe("b");
  });

  it("moves on when the shown session ends", () => {
    focusSession([s("a", "working")], []);
    expect(focusSession([s("c", "working")], [])?.id).toBe("c");
  });
});

describe("keepSelection", () => {
  it("keeps a hand-picked session while nothing new needs you", () => {
    expect(keepSelection([s("a", "working")], [s("a", "done")], [])).toBe(true);
    expect(keepSelection([s("b", "waiting", 5)], [s("b", "waiting", 5)], [])).toBe(true);
  });

  it("drops it when a session starts needing you", () => {
    expect(keepSelection([s("b", "working")], [s("b", "waiting", 7)], [])).toBe(false);
    // Waiting again after an acknowledged alert is a new alert.
    expect(keepSelection([s("b", "waiting", 5)], [s("b", "waiting", 9)], ["b:5"])).toBe(false);
  });

  it("ignores alerts already acknowledged", () => {
    expect(keepSelection([s("b", "working")], [s("b", "waiting", 7)], ["b:7"])).toBe(true);
  });
});
