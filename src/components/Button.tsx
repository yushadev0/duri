import type { CSSProperties, ReactNode } from "react";
import styles from "./Button.module.css";

type Variant = "plain" | "tinted" | "filled";

interface IconButtonProps {
  /**
   * Accessible name. Not shown as a native tooltip: those render over the
   * island and cover neighbouring controls.
   */
  label: string;
  onClick: () => void;
  size?: number;
  variant?: Variant;
  /** Color for tinted/filled variants. */
  tone?: string;
  disabled?: boolean;
  pressed?: boolean;
  children: ReactNode;
}

/** Icon-only control for the island. Never lets clicks reach the island itself. */
export function IconButton({
  label,
  onClick,
  size = 32,
  variant = "plain",
  tone,
  disabled,
  pressed,
  children,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={`${styles.btn} ${styles[variant]}`}
      style={{ width: size, height: size, "--tone": tone } as CSSProperties}
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {children}
    </button>
  );
}

interface TextButtonProps {
  onClick: () => void;
  variant?: Variant;
  tone?: string;
  disabled?: boolean;
  children: ReactNode;
}

export function TextButton({ onClick, variant = "tinted", tone, disabled, children }: TextButtonProps) {
  return (
    <button
      type="button"
      className={`${styles.btn} ${styles.text} ${styles[variant]}`}
      style={{ "--tone": tone } as CSSProperties}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {children}
    </button>
  );
}
