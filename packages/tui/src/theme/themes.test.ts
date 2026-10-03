import { expect, test } from "bun:test";
import { RGBA } from "@opentui/core";

import {
  BUILTIN_THEME_NAMES,
  defaultTheme,
  resolveTheme,
} from "@theme/palettes.ts";
import { renderTheme } from "@theme/providers.tsx";
import { Chrome, type Theme } from "@theme/themes.ts";
import { isPartialTheme, isTheme } from "@theme/validators.ts";

const COMPLETE: Theme = {
  chrome: Chrome.Framed,
  backgroundChrome: "#181825",
  backgroundSelection: "default",
  text: "default",
  textMuted: "ansi:7",
  textDim: "ansi:bright-black",
  accent: "ansi:blue",
  success: "#a6e3a1",
  warning: "ansi:3",
  err: "ansi:red",
};

function indexed(color: unknown): number | null {
  const rgba = color as RGBA;
  return rgba instanceof RGBA && rgba.intent === "indexed" ? rgba.slot : null;
}

test("the default theme takes its accent and status colors from the terminal palette", () => {
  const rendered = renderTheme(defaultTheme);
  expect(indexed(rendered.accent)).toBe(4);
  expect(indexed(rendered.success)).toBe(2);
  expect(indexed(rendered.warning)).toBe(3);
  expect(indexed(rendered.err)).toBe(1);
  expect(indexed(rendered.textDim)).toBe(8);
});

test("the default theme uses the terminal's own foreground, never an unset one", () => {
  const rendered = renderTheme(defaultTheme);
  for (const color of [rendered.text, rendered.textMuted]) {
    expect(color).toBeInstanceOf(RGBA);
    expect((color as RGBA).intent).toBe("default");
  }
});

test("the default theme paints no background, so the terminal shows through", () => {
  expect(defaultTheme.backgroundChrome).toBeUndefined();
  expect(defaultTheme.backgroundSelection).toBeUndefined();
  const rendered = renderTheme(defaultTheme);
  expect(rendered.backgroundChrome).toBeUndefined();
  expect(rendered.backgroundSelection).toBeUndefined();
  expect(defaultTheme.chrome).toBe(Chrome.None);
});

test("every text-like token in the default theme is set explicitly", () => {
  for (const key of [
    "text",
    "textMuted",
    "textDim",
    "accent",
    "success",
    "warning",
    "err",
  ] as const)
    expect(defaultTheme[key]).toBeDefined();
});

test("an unset or unknown theme name resolves to the terminal-native default", () => {
  expect(resolveTheme(undefined)).toBe(defaultTheme);
  expect(resolveTheme({ name: "nope" })).toBe(defaultTheme);
  expect(BUILTIN_THEME_NAMES).toContain("default");
});

test("the named themes keep their fixed hex colors", () => {
  const mocha = renderTheme(resolveTheme({ name: "mocha" }));
  expect(mocha.accent).toBe("#89b4fa");
  expect(mocha.backgroundChrome).toBe("#181825");
});

test("a per-token override can switch one color to a palette entry", () => {
  const theme = resolveTheme({ name: "mocha", overrides: { err: "ansi:red" } });
  const rendered = renderTheme(theme);
  expect(indexed(rendered.err)).toBe(1);
  expect(rendered.accent).toBe("#89b4fa");
});

test("default means the foreground for text tokens and the background for background tokens", () => {
  const rendered = renderTheme(COMPLETE);
  const text = rendered.text as RGBA;
  const background = rendered.backgroundSelection as RGBA;
  expect(text.intent).toBe("default");
  expect(background.intent).toBe("default");
  expect(text.equals(background)).toBe(false);
  expect(rendered.chrome).toBe(Chrome.Framed);
});

test("a complete theme with palette and default tokens is valid", () => {
  expect(isTheme(COMPLETE)).toBe(true);
});

test("a theme with a malformed ansi token is rejected", () => {
  expect(isTheme({ ...COMPLETE, accent: "ansi:purple" })).toBe(false);
  expect(isTheme({ ...COMPLETE, err: "ansi:999" })).toBe(false);
});

test("a theme missing a token, or with a non-string one, is rejected", () => {
  const { accent: _removed, ...incomplete } = COMPLETE;
  expect(isTheme(incomplete)).toBe(false);
  expect(isTheme({ ...COMPLETE, accent: 4 })).toBe(false);
  expect(isTheme({ ...COMPLETE, chrome: "round" })).toBe(false);
  expect(isTheme(null)).toBe(false);
});

test("a partial theme may carry palette tokens but not a malformed or unknown one", () => {
  expect(isPartialTheme({ err: "ansi:red", text: "default" })).toBe(true);
  expect(isPartialTheme({})).toBe(true);
  expect(isPartialTheme({ err: "ansi:chartreuse" })).toBe(false);
  expect(isPartialTheme({ colour: "#fff" })).toBe(false);
  expect(isPartialTheme({ err: 1 })).toBe(false);
});
