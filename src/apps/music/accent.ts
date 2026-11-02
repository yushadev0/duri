/** Picks an accent color from cover art that stays readable on the black island. */

export function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h * 60, s, l];
}

export const NEUTRAL_ACCENT = "hsl(0 0% 88%)";

/**
 * Lifts the color into a band that reads on #000: lightness 58–74% keeps
 * contrast above 4.5:1 for saturated hues, saturation is boosted so it
 * still feels like the artwork. Near-grey artwork gets a neutral accent.
 */
export function islandAccent(r: number, g: number, b: number): string {
  const [h, s, l] = rgbToHsl(r, g, b);
  if (s < 0.14) return NEUTRAL_ACCENT;
  const sat = Math.round(Math.max(s, 0.55) * 100);
  const light = Math.round(Math.min(0.74, Math.max(0.58, l)) * 100);
  return `hsl(${Math.round(h)} ${sat}% ${light}%)`;
}

export async function extractAccent(dataUrl: string): Promise<string> {
  const img = new Image();
  img.src = dataUrl;
  await img.decode();
  const size = 24;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return NEUTRAL_ACCENT;
  ctx.drawImage(img, 0, 0, size, size);
  const { data } = ctx.getImageData(0, 0, size, size);

  // Weight vivid, mid-lightness pixels: they define how a cover "feels".
  let wr = 0, wg = 0, wb = 0, wt = 0;
  let ar = 0, ag = 0, ab = 0;
  const n = data.length / 4;
  for (let i = 0; i < data.length; i += 4) {
    const [r, g, b] = [data[i], data[i + 1], data[i + 2]];
    ar += r;
    ag += g;
    ab += b;
    const [, s, l] = rgbToHsl(r, g, b);
    const w = s * Math.max(0, 1 - Math.abs(l - 0.5) * 1.8);
    wr += r * w;
    wg += g * w;
    wb += b * w;
    wt += w;
  }
  if (wt < n * 0.02) return islandAccent(ar / n, ag / n, ab / n);
  return islandAccent(wr / wt, wg / wt, wb / wt);
}
