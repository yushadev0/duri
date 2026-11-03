import { describe, expect, it } from "vitest";
import { HANDLE, TUCK_DISTANCE, WINDOW_WIDTH, dragOffset, handleHitRect, tuckGesture } from "./geometry";

describe("tuckGesture", () => {
  it("tucks after dragging up far enough", () => {
    expect(tuckGesture(-TUCK_DISTANCE, false)).toBe("tuck");
    expect(tuckGesture(-10, false)).toBeNull();
    expect(tuckGesture(40, false)).toBeNull();
  });

  it("tucks on a short drag that reaches the top edge of the screen", () => {
    expect(tuckGesture(-8, false, true)).toBe("tuck");
    expect(tuckGesture(-3, false, true)).toBeNull();
  });

  it("brings it back after dragging down far enough", () => {
    expect(tuckGesture(TUCK_DISTANCE, true)).toBe("untuck");
    expect(tuckGesture(-40, true)).toBeNull();
  });
});

describe("dragOffset", () => {
  it("follows toward the action and stops at the threshold", () => {
    expect(dragOffset(-10, false)).toBe(-6);
    expect(dragOffset(-200, false)).toBe(-TUCK_DISTANCE);
    expect(dragOffset(200, true)).toBe(TUCK_DISTANCE);
  });

  it("barely moves the other way", () => {
    expect(dragOffset(20, false)).toBe(3);
  });
});

describe("handleHitRect", () => {
  it("is centered, reaches the screen edge and pads the handle", () => {
    const r = handleHitRect(6);
    expect(r.y).toBe(0);
    expect(r.x + r.w / 2).toBe(WINDOW_WIDTH / 2);
    expect(r.w).toBeGreaterThan(HANDLE.w + 40);
  });
});
