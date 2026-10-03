import { type KeySpec, matchKey } from "@input/keys.ts";
import type { KeyEvent } from "@opentui/core";

/** Named actions a key can be bound to. `SelectNext`, `SelectPrev`, `Compose`
 * and `NewThread` are reserved for a future thread-list pane. */
export enum KeyAction {
  FocusNext = "focus_next",
  FocusPrev = "focus_prev",
  SelectNext = "select_next",
  SelectPrev = "select_prev",
  Compose = "compose",
  NewThread = "new_thread",
  NewSession = "new_session",
  Refresh = "refresh",
  Quit = "quit",
  Cancel = "cancel",
}

export const DEFAULT_KEY_BINDINGS: Record<KeyAction, readonly string[]> = {
  [KeyAction.FocusNext]: ["tab"],
  [KeyAction.FocusPrev]: ["shift+tab"],
  [KeyAction.SelectNext]: ["j", "down"],
  [KeyAction.SelectPrev]: ["k", "up"],
  [KeyAction.Compose]: ["i", "return"],
  [KeyAction.NewThread]: ["n"],
  [KeyAction.NewSession]: ["ctrl+n"],
  [KeyAction.Refresh]: ["ctrl+l"],
  [KeyAction.Quit]: ["ctrl+c"],
  [KeyAction.Cancel]: ["escape"],
};

const MODIFIERS = new Set(["ctrl", "shift", "meta"]);

/** `"shift+tab"` -> `{ name: "tab", shift: true }`. Modifiers are only set when
 * present: `matchKey` treats a `false` modifier as required-absent. */
export function parseKeySpec(raw: string): KeySpec {
  const parts = raw
    .trim()
    .split("+")
    .map((part) => part.trim().toLowerCase());
  const name = parts[parts.length - 1] || raw;
  const spec: KeySpec = { name };
  for (const part of parts.slice(0, -1)) {
    if (part === "ctrl") spec.ctrl = true;
    else if (part === "shift") spec.shift = true;
    else if (part === "meta") spec.meta = true;
  }
  return spec;
}

export function isKeyAction(value: string): value is KeyAction {
  return (Object.values(KeyAction) as string[]).includes(value);
}

/** True when `raw` looks like a real key spec string: a bare key name, or
 * one or more `+`-joined modifiers (`ctrl`/`shift`/`meta`) followed by one. */
export function isKeySpecString(raw: unknown): raw is string {
  if (typeof raw !== "string" || raw.trim() === "") return false;
  const parts = raw.trim().toLowerCase().split("+");
  const key = parts.at(-1);
  return (
    parts.slice(0, -1).every((part) => MODIFIERS.has(part)) &&
    key !== undefined &&
    key.length > 0
  );
}

/** `[keybindings]` in `mitos.toml`: action -> key-spec strings. */
export type KeybindingsConfig = Partial<Record<KeyAction, string[]>>;

export type ResolvedKeyBindings = Record<KeyAction, KeySpec[]>;

/** Layers `overrides` over `DEFAULT_KEY_BINDINGS`; an action left out keeps
 * its default. */
export function resolveKeyBindings(
  overrides: KeybindingsConfig | undefined,
): ResolvedKeyBindings {
  const result = {} as ResolvedKeyBindings;
  for (const action of Object.values(KeyAction)) {
    const raw = overrides?.[action] ?? DEFAULT_KEY_BINDINGS[action];
    result[action] = raw.map(parseKeySpec);
  }
  return result;
}

export function matchesAction(
  event: KeyEvent,
  specs: readonly KeySpec[],
): boolean {
  return specs.some((spec) => matchKey(event, spec));
}
