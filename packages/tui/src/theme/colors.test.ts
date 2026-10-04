import { expect, test } from "bun:test";
import { RGBA } from "@opentui/core";

import {
  ansiIndex,
  approximateRgb,
  blend,
  ColorRole,
  isColorToken,
  toColor,
} from "@theme/colors.ts";

test("ansi tokens name a palette entry by number or by name", () => {
  expect(ansiIndex("ansi:0")).toBe(0);
  expect(ansiIndex("ansi:4")).toBe(4);
  expect(ansiIndex("ansi:255")).toBe(255);
  expect(ansiIndex("ansi:blue")).toBe(4);
  expect(ansiIndex("ansi:Bright-Black")).toBe(8);
  expect(ansiIndex("ansi:bright-white")).toBe(15);
});

test("malformed ansi tokens and other strings name no entry", () => {
  expect(ansiIndex("ansi:256")).toBeNull();
  expect(ansiIndex("ansi:-1")).toBeNull();
  expect(ansiIndex("ansi:teal")).toBeNull();
  expect(ansiIndex("ansi:")).toBeNull();
  expect(ansiIndex("#89b4fa")).toBeNull();
  expect(ansiIndex("blue")).toBeNull();
});

test("only a malformed ansi token is rejected as a color token", () => {
  expect(isColorToken("ansi:2")).toBe(true);
  expect(isColorToken("default")).toBe(true);
  expect(isColorToken("#89b4fa")).toBe(true);
  expect(isColorToken("rebeccapurple")).toBe(true);
  expect(isColorToken("ansi:99999")).toBe(false);
  expect(isColorToken("ansi:purple")).toBe(false);
  expect(isColorToken(4)).toBe(false);
  expect(isColorToken(undefined)).toBe(false);
});

test("an ansi token becomes the terminal's own palette color", () => {
  const color = toColor("ansi:green", ColorRole.Foreground);
  expect(color).toBeInstanceOf(RGBA);
  expect((color as RGBA).intent).toBe("indexed");
  expect((color as RGBA).slot).toBe(2);
});

test("default becomes the terminal's default for the role asked", () => {
  const foreground = toColor("default", ColorRole.Foreground) as RGBA;
  const background = toColor("default", ColorRole.Background) as RGBA;
  expect(foreground.intent).toBe("default");
  expect(background.intent).toBe("default");
  expect(foreground.equals(background)).toBe(false);
});

test("hex, css names and undefined pass through untouched", () => {
  expect(toColor("#89b4fa", ColorRole.Foreground)).toBe("#89b4fa");
  expect(toColor("rebeccapurple", ColorRole.Background)).toBe("rebeccapurple");
  expect(toColor(undefined, ColorRole.Foreground)).toBeUndefined();
});

test("approximate rgb gives gradients real channels for every token form", () => {
  expect(approximateRgb("#ff8000")).toEqual([255, 128, 0]);
  expect(approximateRgb("ansi:1")).toEqual([...approximateRgb("ansi:red")]);
  expect(approximateRgb("ansi:2")[1]).toBeGreaterThan(
    approximateRgb("ansi:2")[0],
  );
  expect(approximateRgb("default")).toHaveLength(3);
});

test("blend mixes a foreground into a background by alpha", () => {
  expect(blend("#ffffff", "#000000", 0)).toBe("#000000");
  expect(blend("#ffffff", "#000000", 1)).toBe("#ffffff");
  expect(blend("#ff0000", "#000000", 0.5)).toBe("#800000");
});
