import { describe, expect, it } from "vitest";
import { timeLeft } from "./store";

describe("timeLeft", () => {
  it("splits into hours and minutes", () => {
    expect(timeLeft(3 * 3600 + 20 * 60)).toEqual({ h: 3, m: 20 });
    expect(timeLeft(45 * 60 + 20)).toEqual({ h: 0, m: 45 });
  });

  it("is unknown without an estimate", () => {
    expect(timeLeft(null)).toBeNull();
    expect(timeLeft(0)).toBeNull();
  });
});
