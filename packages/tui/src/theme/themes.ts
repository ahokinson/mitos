export enum Chrome {
  None = "none",
  Framed = "framed",
}

/** Semantic color roles, resolved to a theme color at render time. */
export enum Tone {
  Accent = "accent",
  Success = "success",
  Warning = "warning",
  Error = "error",
  Muted = "muted",
  Dim = "dim",
}

export type Theme = {
  backgroundChrome?: string;
  backgroundSelection?: string;
  text?: string;
  textMuted?: string;
  textDim?: string;
  diffText?: string;
  accent?: string;
  success?: string;
  warning?: string;
  err?: string;
  chrome: Chrome;
};

/** A built-in or `[themes]` custom theme name, plus per-token overrides. */
export type ThemeConfig = {
  name?: string;
  overrides?: Partial<Theme>;
};

/** `[themes]` in `mitos.toml`: custom theme name -> file path as written. */
export type ThemePathsConfig = Record<string, string>;

/** opentui's `attributes` bit mask for bold text. */
export const BOLD = 1;
