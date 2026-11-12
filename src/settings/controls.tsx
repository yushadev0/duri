import type { ReactNode } from "react";
import styles from "./Settings.module.css";

export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={styles.switch}
      onClick={() => onChange(!checked)}
    >
      <span className={styles.knob} />
    </button>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className={styles.segmented} role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          className={styles.segment}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Row({ title, hint, children, icon }: { title: string; hint?: string; children?: ReactNode; icon?: ReactNode }) {
  return (
    <div className={styles.row}>
      {icon}
      <div className={styles.rowText}>
        <span className={styles.rowTitle}>{title}</span>
        {hint && <span className={styles.rowHint}>{hint}</span>}
      </div>
      {children}
    </div>
  );
}
