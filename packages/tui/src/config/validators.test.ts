import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";

import {
  validateHandoff,
  validateKeybindings,
  validateSession,
  validateThemeConfig,
  validateThemePaths,
} from "@config/validators.ts";
import { KnownHarness } from "@harness/harnesses.ts";
import { KeyAction } from "@input/keybindings.ts";

let warnSpy: ReturnType<typeof spyOn>;

beforeEach(() => {
  warnSpy = spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  warnSpy.mockRestore();
});

test("theme config accepts a name and valid overrides", () => {
  expect(
    validateThemeConfig({ name: "mocha", overrides: { accent: "#ffffff" } }),
  ).toEqual({ name: "mocha", overrides: { accent: "#ffffff" } });
  expect(warnSpy).not.toHaveBeenCalled();
});

test("theme config tolerates absence", () => {
  expect(validateThemeConfig(undefined)).toEqual({});
  expect(warnSpy).not.toHaveBeenCalled();
});

test("theme config rejects a non-table", () => {
  expect(validateThemeConfig("mocha")).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(1);
});

test("theme config drops a non-string name and invalid overrides", () => {
  expect(
    validateThemeConfig({ name: 3, overrides: { accent: "ansi:nope" } }),
  ).toEqual({ name: undefined, overrides: undefined });
  expect(warnSpy).toHaveBeenCalledTimes(2);
});

test("theme config treats a null table value as absent", () => {
  expect(validateThemeConfig(null)).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(1);
});

test("theme paths keep only string entries", () => {
  expect(validateThemePaths({ a: "a.toml", b: 4 })).toEqual({ a: "a.toml" });
  expect(validateThemePaths(undefined)).toEqual({});
  expect(warnSpy).not.toHaveBeenCalled();
});

test("theme paths reject a non-table", () => {
  expect(validateThemePaths([])).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(1);
});

test("session accepts a known harness", () => {
  expect(validateSession({ default_harness: "codex" })).toEqual({
    defaultHarness: KnownHarness.Codex,
  });
});

test("session ignores absence and invalid values", () => {
  expect(validateSession(undefined)).toEqual({});
  expect(validateSession({})).toEqual({});
  expect(warnSpy).not.toHaveBeenCalled();
  expect(validateSession({ default_harness: "nope" })).toEqual({});
  expect(validateSession("codex")).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(2);
});

test("handoff accepts a positive integer", () => {
  expect(validateHandoff({ max_inline_bytes: 1024 })).toEqual({
    maxInlineBytes: 1024,
  });
});

test("handoff ignores absence and invalid values", () => {
  expect(validateHandoff(undefined)).toEqual({});
  expect(validateHandoff({})).toEqual({});
  expect(warnSpy).not.toHaveBeenCalled();
  expect(validateHandoff({ max_inline_bytes: 0 })).toEqual({});
  expect(validateHandoff({ max_inline_bytes: 1.5 })).toEqual({});
  expect(validateHandoff({ max_inline_bytes: "9" })).toEqual({});
  expect(validateHandoff(7)).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(4);
});

test("keybindings keep valid actions and warn on the rest", () => {
  expect(
    validateKeybindings({
      quit: ["ctrl+q"],
      refresh: [],
      bogus: ["x"],
      cancel: ["ctrl+"],
      compose: "i",
    }),
  ).toEqual({ [KeyAction.Quit]: ["ctrl+q"] });
  expect(warnSpy).toHaveBeenCalledTimes(4);
});

test("keybindings tolerate absence and reject a non-table", () => {
  expect(validateKeybindings(undefined)).toEqual({});
  expect(warnSpy).not.toHaveBeenCalled();
  expect(validateKeybindings(["quit"])).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(1);
});
