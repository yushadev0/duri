interface RingProps {
  /** 0…1 of the ring that is filled. */
  progress: number;
  size: number;
  stroke?: number;
  color: string;
  children?: React.ReactNode;
}

/** Circular progress, used by the Timer app (fills clockwise from 12 o'clock). */
export function Ring({ progress, size, stroke = 3, color, children }: RingProps) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const p = Math.min(1, Math.max(0, progress));
  return (
    <span style={{ position: "relative", display: "inline-grid", placeItems: "center", width: size, height: size, flex: "none" }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden style={{ position: "absolute", inset: 0 }}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeOpacity={0.25} strokeWidth={stroke} />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - p)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
      </svg>
      {children}
    </span>
  );
}
