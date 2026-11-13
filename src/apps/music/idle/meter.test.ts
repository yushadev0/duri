import { describe, expect, it } from "vitest";
import { follow, type Envelope } from "./meter";

const run = (env: Envelope, raw: number, ms: number) => {
  for (let t = 0; t < ms; t += 16) env = follow(env, raw, 16);
  return env;
};

describe("follow", () => {
  it("rises fast and falls slowly", () => {
    const up = run({ peak: 0.5, value: 0 }, 0.5, 120);
    expect(up.value).toBeGreaterThan(0.85);
    const down = run(up, 0, 120);
    expect(down.value).toBeGreaterThan(0.6);
    expect(run(down, 0, 2000).value).toBeLessThan(0.05);
  });

  it("scales a quiet song up to its own peak", () => {
    expect(run({ peak: 0.04, value: 0 }, 0.12, 500).value).toBeGreaterThan(0.95);
  });

  it("keeps silence silent", () => {
    expect(run({ peak: 0.04, value: 0 }, 0.01, 500).value).toBeLessThan(0.3);
  });
});
