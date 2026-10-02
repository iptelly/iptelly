export interface KeyboardShortcut {
  // One or more combos like "ctrl + f", "cmd + a" or "left". Modifiers must
  // match exactly, so "left" doesn't fire for shift + left.
  keys: string[];
  label: string;
  description: string;
  preventDefault?: boolean;
  // Shortcuts are ignored while a text field, select or textarea has focus,
  // unless this is set (it allows <input> only, not textarea/select).
  allowInInput?: boolean;
  command: () => unknown;
}

const KEY_ALIASES: Record<string, string> = {
  left: "arrowleft",
  right: "arrowright",
  up: "arrowup",
  down: "arrowdown",
  space: " ",
  esc: "escape",
};

function comboMatches(combo: string, event: KeyboardEvent): boolean {
  const parts = combo.split("+").map((p) => p.trim().toLowerCase());
  const key = parts.pop()!;
  return (
    event.key.toLowerCase() === (KEY_ALIASES[key] ?? key) &&
    event.ctrlKey === parts.includes("ctrl") &&
    event.metaKey === parts.includes("cmd") &&
    event.altKey === parts.includes("alt") &&
    event.shiftKey === parts.includes("shift")
  );
}

function blockedByFocus(shortcut: KeyboardShortcut, event: KeyboardEvent): boolean {
  const target = event.target as HTMLElement | null;
  if (!target) return false;
  const tag = target.tagName?.toLowerCase();
  if (tag === "input") return !shortcut.allowInInput;
  return tag === "textarea" || tag === "select" || target.isContentEditable;
}

// Runs the first shortcut matching the event. Returns true if one ran.
export function runKeyboardShortcut(shortcuts: KeyboardShortcut[], event: KeyboardEvent): boolean {
  const shortcut = shortcuts.find((s) => s.keys.some((combo) => comboMatches(combo, event)));
  if (!shortcut || blockedByFocus(shortcut, event)) return false;
  if (shortcut.preventDefault) event.preventDefault();
  shortcut.command();
  return true;
}
