import { describe, expect, it } from "vitest";
import { newlyStarted, watched, type InUse } from "./store";

const none: InUse = { camera: [], microphone: [] };
const all = () => true;

describe("newlyStarted", () => {
  it("names the app that just started", () => {
    expect(newlyStarted(none, { camera: [], microphone: ["Discord"] }, all)).toEqual({ device: "microphone", name: "Discord" });
  });

  it("prefers the camera when both start at once", () => {
    expect(newlyStarted(none, { camera: ["Zoom"], microphone: ["Zoom"] }, all)).toEqual({ device: "camera", name: "Zoom" });
  });

  it("stays quiet when an app stops or keeps going", () => {
    const before: InUse = { camera: ["Zoom"], microphone: ["Zoom"] };
    expect(newlyStarted(before, { camera: [], microphone: ["Zoom"] }, all)).toBeNull();
    expect(newlyStarted(before, before, all)).toBeNull();
  });

  it("skips devices the person turned off", () => {
    const settings = { privacyCamera: false, privacyMicrophone: true };
    const after: InUse = { camera: ["Zoom"], microphone: [] };
    expect(newlyStarted(none, after, (d) => watched(d, settings))).toBeNull();
  });
});
