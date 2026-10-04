import type { ColorInput, SyntaxStyle } from "@opentui/core"
import { type ParentProps, createContext, createMemo, onCleanup, useContext } from "solid-js"

import { ColorRole, toColor } from "@theme/colors.ts"
import { defaultTheme } from "@theme/palettes.ts"
import { syntaxStyleFor } from "@theme/syntaxes.ts"
import type { Chrome, Theme } from "@theme/themes.ts"

/** A theme as the renderer takes it: palette and default tokens are already
 * OpenTui colors, so widgets pass them straight to `fg` and `backgroundColor`. */
export type RenderTheme = { chrome: Chrome } & {
  [Key in Exclude<keyof Theme, "chrome">]?: ColorInput
}

const BACKGROUND_TOKENS: ReadonlySet<string> = new Set([
  "backgroundChrome",
  "backgroundSelection",
])

export function renderTheme(theme: Theme): RenderTheme {
  return Object.fromEntries(
    Object.entries(theme).map(([key, token]) => [
      key,
      key === "chrome"
        ? token
        : toColor(
            token as string | undefined,
            BACKGROUND_TOKENS.has(key) ? ColorRole.Background : ColorRole.Foreground,
          ),
    ]),
  ) as RenderTheme
}

type ThemeValue = { tokens: Theme; render: RenderTheme; syntax: SyntaxStyle }

function themeValue(theme: Theme): ThemeValue {
  return { tokens: theme, render: renderTheme(theme), syntax: syntaxStyleFor(theme) }
}

const ThemeContext = createContext<ThemeValue>(themeValue(defaultTheme))

export function ThemeProvider(props: ParentProps & { theme?: Theme }) {
  const value = createMemo(() => themeValue(props.theme ?? defaultTheme))
  onCleanup(() => value().syntax.destroy())
  return <ThemeContext.Provider value={value()}>{props.children}</ThemeContext.Provider>
}

/** Highlight styles for code, diffs and markdown, in the active theme. */
export function useSyntaxStyle(): SyntaxStyle {
  return useContext(ThemeContext).syntax
}

export function useTheme(): RenderTheme {
  return useContext(ThemeContext).render
}

/** The theme as written, for code that must read a token rather than paint
 * with it, such as template gradients. */
export function useThemeTokens(): Theme {
  return useContext(ThemeContext).tokens
}
