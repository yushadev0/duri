/**
 * Duri's mark: a durian, reduced to a rounded spiky husk with a split showing
 * the soft fruit inside. Kept as inline SVG so it scales crisply anywhere.
 */
export function DuriMark({ size = 64 }: { size?: number }) {
  const spikes = Array.from({ length: 14 }, (_, i) => {
    const a = (i / 14) * Math.PI * 2;
    const r1 = 40;
    const r2 = 47;
    const w = 0.13;
    const p = (r: number, ang: number) => `${50 + r * Math.cos(ang)},${54 + r * Math.sin(ang)}`;
    return `M${p(r1, a - w)} L${p(r2, a)} L${p(r1, a + w)} Z`;
  }).join(" ");

  return (
    <svg width={size} height={size} viewBox="0 0 100 100" role="img" aria-label="Duri">
      <path d={spikes} fill="#5f8f12" />
      <circle cx="50" cy="54" r="41" fill="#7aa81c" />
      <circle cx="50" cy="54" r="41" fill="url(#duri-shade)" />
      <path d="M50 22 C 38 40, 38 70, 50 88 C 62 70, 62 40, 50 22 Z" fill="#f6d55c" />
      <path d="M50 30 C 43 44, 43 66, 50 80" stroke="#e9b93a" strokeWidth="3" fill="none" strokeLinecap="round" />
      <path d="M50 14 q 3 -8 9 -9" stroke="#4c6b10" strokeWidth="4" fill="none" strokeLinecap="round" />
      <defs>
        <radialGradient id="duri-shade" cx="0.35" cy="0.3" r="0.8">
          <stop offset="0" stopColor="#fff" stopOpacity="0.25" />
          <stop offset="1" stopColor="#000" stopOpacity="0.2" />
        </radialGradient>
      </defs>
    </svg>
  );
}
