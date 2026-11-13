import { describe, expect, it } from "vitest";
import { buildTimeline, indexAt, parseLrc, plainTimeline, timelineFrom, wordKey, wordLinesToRaw } from "./lyrics";

describe("parseLrc", () => {
  it("reads line times, repeats and skips metadata", () => {
    const raw = parseLrc("[ar:Someone]\n[00:12.50]Hello world\n[00:05.00][01:00.00]Chorus line\n\n[00:20.000]");
    expect(raw.map((l) => [l.t, l.text])).toEqual([
      [5000, "Chorus line"],
      [12500, "Hello world"],
      [20000, ""],
      [60000, "Chorus line"],
    ]);
  });

  it("applies the offset tag", () => {
    expect(parseLrc("[offset:+500]\n[00:01.00]a")[0].t).toBe(500);
  });

  it("keeps enhanced word times", () => {
    const [line] = parseLrc("[00:10.00]<00:10.00>Hey <00:10.80>you <00:11.50>there");
    expect(line.text).toBe("Hey you there");
    expect(line.wordTimes).toEqual([10000, 10800, 11500]);
  });
});

describe("wordKey", () => {
  it("drops case and surrounding punctuation only", () => {
    expect(wordKey("Love,")).toBe("love");
    expect(wordKey("(don't)")).toBe("don't");
    expect(wordKey("Gönül!")).toBe("gönül");
    expect(wordKey("—")).toBe("");
  });
});

describe("buildTimeline", () => {
  it("spreads words through the line in order and marks line endings", () => {
    const tl = buildTimeline(parseLrc("[00:01.00]one two three\n[00:04.00]four — five"), 10_000);
    expect(tl.words.map((w) => w.key)).toEqual(["one", "two", "three", "four", "five"]);
    const ts = tl.words.map((w) => w.t);
    expect(ts[0]).toBe(1000);
    expect(ts).toEqual([...ts].sort((a, b) => a - b));
    expect(ts[2]).toBeLessThan(4000);
    expect(tl.words.filter((w) => w.last).map((w) => w.key)).toEqual(["three", "five"]);
    expect(tl.words[0].end).toBe(ts[1]);
    expect(tl.wordTimed).toBe(false);
  });

  it("uses word times when every line has them", () => {
    const tl = buildTimeline(parseLrc("[00:01.00]<00:01.00>a <00:01.50>b\n[00:03.00]<00:03.00>c"), 5000);
    expect(tl.wordTimed).toBe(true);
    expect(tl.words.map((w) => w.t)).toEqual([1000, 1500, 3000]);
  });

  it("ends a short line early before a long gap", () => {
    const tl = buildTimeline(parseLrc("[00:01.00]hi\n[00:30.00]back"), 40_000);
    expect(tl.lines[0].end).toBeLessThan(3000);
  });
});

describe("word-synced lyrics", () => {
  const lines = [
    {
      text: "Kim olduğumu, ne olduğumu",
      startMs: 30550,
      endMs: 37220,
      words: [
        { text: "Kim ", startMs: 30550 },
        { text: "ol", startMs: 31027 },
        { text: "duğumu, ", startMs: 31300 },
        { text: "ne ", startMs: 33751 },
        { text: "olduğumu", startMs: 34248 },
      ],
    },
  ];

  it("joins syllables into words and keeps their times", () => {
    const [raw] = wordLinesToRaw(lines);
    expect(raw.words).toEqual(["Kim", "olduğumu,", "ne", "olduğumu"]);
    expect(raw.wordTimes).toEqual([30550, 31027, 33751, 34248]);
  });

  it("wins over the LRC and uses the real line end", () => {
    const tl = timelineFrom({ found: true, instrumental: false, synced: "[00:30.55]Kim olduğumu", plain: null, wordLines: lines }, 0)!;
    expect(tl.wordTimed).toBe(true);
    expect(tl.words.map((w) => w.t)).toEqual([30550, 31027, 33751, 34248]);
    expect(tl.lines[0].end).toBe(37220);
  });
});

describe("timelines", () => {
  it("prefers synced lyrics and falls back to plain ones", () => {
    expect(timelineFrom({ found: true, instrumental: false, synced: "[00:01.00]a b", plain: "a b" }, 0)?.synced).toBe(true);
    const plain = timelineFrom({ found: true, instrumental: false, synced: null, plain: "a b\n\n\n\nc" }, 0)!;
    expect(plain.synced).toBe(false);
    expect(plain.lines.map((l) => l.text)).toEqual(["a b", "", "c"]);
    expect(plainTimeline("").words).toEqual([]);
  });

  it("finds the current item", () => {
    const items = [{ t: 0 }, { t: 10 }, { t: 20 }];
    expect(indexAt(items, -1)).toBe(-1);
    expect(indexAt(items, 10)).toBe(1);
    expect(indexAt(items, 99)).toBe(2);
  });
});
