/**
 * Global shortcuts are stored as accelerators the backend parses, e.g.
 * "Ctrl+Alt+Shift+KeyN" (modifiers, then a `KeyboardEvent.code`).
 */

export type ShortcutAction = "island" | "note" | "clipboard";

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_CODES = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/;
/** Keys the backend can register (see global-hotkey's parser). */
const SUPPORTED = /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Enter|Tab|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Arrow(Up|Down|Left|Right)|Home|End|PageUp|PageDown|Insert|Numpad[0-9])$/;

/**
 * The accelerator for a key press, or null while only modifiers are held or
 * when it has no Ctrl, Alt or Win (a plain letter would steal typing).
 */
export function acceleratorFromEvent(e: KeyLike): string | null {
  if (MODIFIER_CODES.test(e.code) || !SUPPORTED.test(e.code)) return null;
  if (!e.ctrlKey && !e.altKey && !e.metaKey) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  parts.push(e.code);
  return parts.join("+");
}

const KEY_NAMES: Record<string, string> = {
  Space: "Space",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/** "Ctrl+Alt+Shift+KeyN" → ["Ctrl", "Alt", "Shift", "N"], for display. */
export function acceleratorKeys(accel: string): string[] {
  if (!accel) return [];
  return accel.split("+").map((p) => {
    if (p === "Super") return "Win";
    if (/^Key[A-Z]$/.test(p)) return p.slice(3);
    if (/^Digit[0-9]$/.test(p)) return p.slice(5);
    if (/^Numpad[0-9]$/.test(p)) return `Num ${p.slice(6)}`;
    return KEY_NAMES[p] ?? p;
  });
}
