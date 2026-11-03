import { describe, expect, it } from "vitest";
import { acceleratorFromEvent, acceleratorKeys } from "./shortcuts";

const key = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("acceleratorFromEvent", () => {
  it("builds modifiers in a fixed order, then the key code", () => {
    expect(acceleratorFromEvent(key("KeyN", { shiftKey: true, ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+Shift+KeyN");
    expect(acceleratorFromEvent(key("Space", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+Space");
    expect(acceleratorFromEvent(key("Digit1", { metaKey: true }))).toBe("Super+Digit1");
  });

  it("waits while only modifiers are held", () => {
    expect(acceleratorFromEvent(key("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(acceleratorFromEvent(key("ShiftRight", { shiftKey: true, ctrlKey: true }))).toBeNull();
  });

  it("needs Ctrl, Alt or Win so typing still works", () => {
    expect(acceleratorFromEvent(key("KeyA"))).toBeNull();
    expect(acceleratorFromEvent(key("KeyA", { shiftKey: true }))).toBeNull();
  });
});

describe("acceleratorKeys", () => {
  it("shows friendly key names", () => {
    expect(acceleratorKeys("Ctrl+Alt+Shift+KeyN")).toEqual(["Ctrl", "Alt", "Shift", "N"]);
    expect(acceleratorKeys("Super+ArrowUp")).toEqual(["Win", "↑"]);
    expect(acceleratorKeys("")).toEqual([]);
  });
});
