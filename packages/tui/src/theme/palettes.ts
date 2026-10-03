import { Chrome, type Theme, type ThemeConfig } from "@theme/themes.ts";

/** Terminal-native: text is the terminal's default foreground, backgrounds
 * are left unset so the terminal shows through, and the accent and status
 * colors are its ANSI palette, so they follow whatever color scheme the
 * terminal uses. Text must be explicit: an unset foreground renders as fixed
 * white, not as the terminal's. */
export const defaultTheme: Theme = {
  chrome: Chrome.None,
  text: "default",
  textMuted: "default",
  textDim: "ansi:bright-black",
  accent: "ansi:blue",
  success: "ansi:green",
  warning: "ansi:yellow",
  err: "ansi:red",
};

/** Catppuccin Mocha. */
const mochaTheme: Theme = {
  chrome: Chrome.Framed,
  backgroundChrome: "#181825",
  backgroundSelection: "#313244",
  text: "#cdd6f4",
  textMuted: "#a6adc8",
  textDim: "#6c7086",
  accent: "#89b4fa",
  success: "#a6e3a1",
  warning: "#f9e2af",
  err: "#f38ba8",
};

/** Catppuccin Latte. */
const latteTheme: Theme = {
  chrome: Chrome.Framed,
  backgroundChrome: "#e6e9ef",
  backgroundSelection: "#ccd0da",
  text: "#4c4f69",
  textMuted: "#6c6f85",
  textDim: "#9ca0b0",
  accent: "#1e66f5",
  success: "#40a02b",
  warning: "#df8e1d",
  err: "#d20f39",
};

/** Rosé Pine Moon. The palette has no green, so `success` borrows Foam. */
const rosePineMoonTheme: Theme = {
  chrome: Chrome.Framed,
  backgroundChrome: "#2a273f",
  backgroundSelection: "#393552",
  text: "#e0def4",
  textMuted: "#908caa",
  textDim: "#6e6a86",
  accent: "#3e8fb0",
  success: "#9ccfd8",
  warning: "#f6c177",
  err: "#eb6f92",
};

const BUILTIN_THEMES: Record<string, Theme> = {
  default: defaultTheme,
  mocha: mochaTheme,
  latte: latteTheme,
  "rose-pine-moon": rosePineMoonTheme,
};

export const BUILTIN_THEME_NAMES: readonly string[] =
  Object.keys(BUILTIN_THEMES);

export const DEFAULT_THEME_NAME = "default";

/** Custom themes shadow built-ins by name; an unknown name falls back to the default. */
export function resolveTheme(
  config: ThemeConfig | undefined,
  customThemes: Record<string, Theme> = {},
): Theme {
  const selected = config?.name ?? DEFAULT_THEME_NAME;
  const base =
    customThemes[selected] ?? BUILTIN_THEMES[selected] ?? defaultTheme;
  return config?.overrides ? { ...base, ...config.overrides } : base;
}
