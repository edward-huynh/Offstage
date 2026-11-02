/** Key names that differ between the web's `KeyboardEvent.code` and a Tauri accelerator. */
const KEY_NAMES: Record<string, string> = {
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** The modifier keys themselves: pressing one alone is not yet a shortcut. */
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

/**
 * The accelerator ("Control+Alt+P") for a key press, or `null` while only modifiers are held.
 * Uses the physical key, so Option+P on a Mac is Alt+P and not the character it types. Rust
 * decides whether the result is a shortcut it accepts.
 */
export function acceleratorFromEvent(event: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(event.code)) return null;
  const letter = /^Key([A-Z])$/.exec(event.code);
  const digit = /^Digit(\d)$/.exec(event.code);
  const key = letter?.[1] ?? digit?.[1] ?? KEY_NAMES[event.code] ?? event.code;
  if (!key) return null;
  const modifiers = [
    event.ctrlKey && "Control",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter(Boolean);
  return [...modifiers, key].join("+");
}

/**
 * A shortcut as shown to the user: ⌘ ⌥ ⇧ ⌃ on macOS, Ctrl Alt Shift Win elsewhere.
 * Understands Tauri's `CommandOrControl` as well as the plain modifier names.
 */
export function formatAccelerator(accelerator: string): string {
  const mac = document.documentElement.dataset.platform === "mac";
  const arrows = { Up: "↑", Down: "↓", Left: "←", Right: "→" };
  const symbols: Record<string, string> = mac
    ? { CommandOrControl: "⌘", Control: "⌃", Ctrl: "⌃", Super: "⌘", Alt: "⌥", Shift: "⇧", ...arrows }
    : { CommandOrControl: "Ctrl", Control: "Ctrl", Super: "Win", ...arrows };
  return accelerator
    .split("+")
    .map((key) => symbols[key] ?? key)
    .join(mac ? " " : " + ");
}
