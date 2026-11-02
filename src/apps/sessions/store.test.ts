import { describe, expect, it } from "vitest";
import { focusSession, type Session } from "./store";

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
