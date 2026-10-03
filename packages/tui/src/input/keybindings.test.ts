import { expect, test } from "bun:test";
import {
  DEFAULT_KEY_BINDINGS,
  isKeyAction,
  isKeySpecString,
  KeyAction,
  matchesAction,
  parseKeySpec,
  resolveKeyBindings,
} from "@input/keybindings.ts";
import type { KeyEvent } from "@opentui/core";

function key(overrides: Partial<KeyEvent>): KeyEvent {
  return {
    name: "",
    sequence: "",
    ctrl: false,
    meta: false,
    shift: false,
    ...overrides,
  } as KeyEvent;
}

test("parseKeySpec parses a bare key", () => {
  expect(parseKeySpec("j")).toEqual({ name: "j" });
});

test("parseKeySpec parses a single modifier", () => {
  expect(parseKeySpec("shift+tab")).toEqual({ name: "tab", shift: true });
});

test("parseKeySpec parses multiple modifiers and lowercases", () => {
  expect(parseKeySpec("Ctrl+Shift+C")).toEqual({
    name: "c",
    ctrl: true,
    shift: true,
  });
});

test("parseKeySpec never sets a modifier to false explicitly", () => {
  const spec = parseKeySpec("j");
  expect(Object.keys(spec)).toEqual(["name"]);
});

test("isKeyAction recognizes every default action and rejects unknown strings", () => {
  for (const action of Object.values(KeyAction))
    expect(isKeyAction(action)).toBe(true);
  expect(isKeyAction("not_a_real_action")).toBe(false);
});

test("isKeySpecString accepts bare keys and modifier combos, rejects garbage", () => {
  expect(isKeySpecString("j")).toBe(true);
  expect(isKeySpecString("shift+tab")).toBe(true);
  expect(isKeySpecString("ctrl+shift+p")).toBe(true);
  expect(isKeySpecString("")).toBe(false);
  expect(isKeySpecString("banana+tab")).toBe(false);
  expect(isKeySpecString(42)).toBe(false);
});

test("resolveKeyBindings falls back to defaults with no overrides", () => {
  const resolved = resolveKeyBindings(undefined);
  expect(resolved[KeyAction.Quit]).toEqual([{ name: "c", ctrl: true }]);
  expect(resolved[KeyAction.FocusPrev]).toEqual([{ name: "tab", shift: true }]);
});

test("resolveKeyBindings overrides one action, keeps the rest default", () => {
  const resolved = resolveKeyBindings({ [KeyAction.Quit]: ["ctrl+c"] });
  expect(resolved[KeyAction.Quit]).toEqual([{ name: "c", ctrl: true }]);
  expect(resolved[KeyAction.SelectNext]).toEqual(
    DEFAULT_KEY_BINDINGS[KeyAction.SelectNext].map(parseKeySpec),
  );
});

test("matchesAction matches any of several specs for an action", () => {
  const specs = resolveKeyBindings(undefined)[KeyAction.SelectNext];
  expect(matchesAction(key({ name: "j" }), specs)).toBe(true);
  expect(matchesAction(key({ name: "down" }), specs)).toBe(true);
  expect(matchesAction(key({ name: "k" }), specs)).toBe(false);
});

test("matchesAction respects modifiers", () => {
  const specs = resolveKeyBindings(undefined)[KeyAction.FocusPrev];
  expect(matchesAction(key({ name: "tab", shift: true }), specs)).toBe(true);
  expect(matchesAction(key({ name: "tab", shift: false }), specs)).toBe(false);
});
