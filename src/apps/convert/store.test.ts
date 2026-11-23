import { describe, expect, it } from "vitest";
import { commonTargets, formatBytes, formatClock, parseClock, secondsLeft, type Job, type Probed } from "./store";

const file = (kind: Probed["media"]["kind"], targets: string[], error: string | null = null): Probed => ({
  path: "p",
  name: "n",
  bytes: 1,
  media: { kind, duration: null, width: null, height: null, hasAudio: false },
  targets: targets.map((id) => ({ id, output: "video", lossless: false })),
  error,
});

describe("converter store", () => {
  it("offers only formats every file can become", () => {
    const video = file("video", ["mp4", "webm", "mp3", "wav"]);
    const song = file("audio", ["mp3", "m4a", "wav"]);
    expect(commonTargets([video, song]).map((t) => t.id)).toEqual(["mp3", "wav"]);
    // Files ffprobe didn't understand don't narrow the choice.
    expect(commonTargets([video, file(null, [], "unsupported")]).map((t) => t.id)).toEqual(["mp4", "webm", "mp3", "wav"]);
    expect(commonTargets([])).toEqual([]);
  });

  it("reads and writes clock times", () => {
    expect(parseClock("1:23")).toBe(83);
    expect(parseClock("83")).toBe(83);
    expect(parseClock("1:02:03")).toBe(3723);
    expect(parseClock("0:05,5")).toBe(5.5);
    expect(parseClock("")).toBeNull();
    expect(parseClock("abc")).toBeNull();
    expect(parseClock("1:2:3:4")).toBeNull();
    expect(formatClock(83)).toBe("1:23");
    expect(formatClock(3723)).toBe("1:02:03");
  });

  it("formats sizes", () => {
    expect(formatBytes(66_513_201)).toBe("67 MB");
    expect(formatBytes(830_000)).toBe("830 KB");
    expect(formatBytes(2_400_000_000)).toBe("2.4 GB");
  });

  it("guesses the time left once there is progress", () => {
    const job: Job = {
      id: 1, input: "", name: "", target: "mp4", status: "running", progress: 0.25,
      output: null, error: null, startedAt: 0,
    };
    expect(secondsLeft(job, 10_000)).toBe(30);
    expect(secondsLeft({ ...job, progress: 0.01 }, 10_000)).toBeNull();
    expect(secondsLeft({ ...job, status: "done" }, 10_000)).toBeNull();
  });
});
