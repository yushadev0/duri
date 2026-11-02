let ctx: AudioContext | null = null;

/** A short, soft two-note chime played three times (timer finished). */
export function playChime() {
  try {
    ctx ??= new AudioContext();
    const start = ctx.currentTime + 0.02;
    for (let r = 0; r < 3; r++) {
      [880, 1318.5].forEach((freq, i) => {
        const t = start + r * 0.9 + i * 0.16;
        const osc = ctx!.createOscillator();
        const gain = ctx!.createGain();
        osc.type = "sine";
        osc.frequency.value = freq;
        gain.gain.setValueAtTime(0, t);
        gain.gain.linearRampToValueAtTime(0.18, t + 0.02);
        gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.7);
        osc.connect(gain).connect(ctx!.destination);
        osc.start(t);
        osc.stop(t + 0.75);
      });
    }
  } catch {
    // Audio is a nice-to-have; the visual alert and notification still happen.
  }
}
