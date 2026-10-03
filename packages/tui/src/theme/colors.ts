import {
  ansi256IndexToRgb,
  type ColorInput,
  DEFAULT_FOREGROUND_RGB,
  RGBA,
  type RGBTriplet,
} from "@opentui/core";

const ANSI_PREFIX = "ansi:";
const DEFAULT_TOKEN = "default";
const PALETTE_SIZE = 256;

/** The 16 named palette entries, in the terminal's own order. */
const ANSI_NAMES: Record<string, number> = {
  black: 0,
  red: 1,
  green: 2,
  yellow: 3,
  blue: 4,
  magenta: 5,
  cyan: 6,
  white: 7,
  "bright-black": 8,
  "bright-red": 9,
  "bright-green": 10,
  "bright-yellow": 11,
  "bright-blue": 12,
  "bright-magenta": 13,
  "bright-cyan": 14,
  "bright-white": 15,
};

export enum ColorRole {
  Foreground = "foreground",
  Background = "background",
}

/** The palette index an `ansi:` token names, or `null` for anything else,
 * including a malformed `ansi:` token. */
export function ansiIndex(token: string): number | null {
  if (!token.startsWith(ANSI_PREFIX)) return null;
  const name = token.slice(ANSI_PREFIX.length).toLowerCase();
  if (name in ANSI_NAMES) return ANSI_NAMES[name] ?? null;
  if (!/^\d+$/.test(name)) return null;
  const index = Number(name);
  return index < PALETTE_SIZE ? index : null;
}

/** Hex and CSS names pass through to OpenTui unchanged, so only the forms
 * Mitos itself defines can be wrong. */
export function isColorToken(value: unknown): value is string {
  if (typeof value !== "string") return false;
  return !value.startsWith(ANSI_PREFIX) || ansiIndex(value) !== null;
}

/** `ansi:N` follows the terminal's own palette and `default` is the terminal's
 * default foreground or background, so neither is a fixed RGB value. Any
 * other token is left for OpenTui to parse. */
export function toColor(
  token: string | undefined,
  role: ColorRole,
): ColorInput | undefined {
  if (token === undefined) return undefined;
  if (token === DEFAULT_TOKEN)
    return role === ColorRole.Background
      ? RGBA.defaultBackground()
      : RGBA.defaultForeground();
  const index = ansiIndex(token);
  return index === null ? token : RGBA.fromIndex(index);
}

/** An approximate RGB for a token, for gradients, which must interpolate real
 * channels. A palette entry uses the xterm default for its slot, so a
 * gradient between palette colors is close to, not exactly, the terminal's. */
export function approximateRgb(token: string): RGBTriplet {
  if (token === DEFAULT_TOKEN) return DEFAULT_FOREGROUND_RGB;
  const index = ansiIndex(token);
  if (index !== null) return ansi256IndexToRgb(index);
  const [r, g, b] = RGBA.fromHex(token).toInts();
  return [r, g, b];
}
