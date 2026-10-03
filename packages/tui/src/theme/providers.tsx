import type { ColorInput } from "@opentui/core"
import { type ParentProps, createContext, useContext } from "solid-js"

import { ColorRole, toColor } from "@theme/colors.ts"
import { defaultTheme } from "@theme/palettes.ts"
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

type ThemeValue = { tokens: Theme; render: RenderTheme }

function themeValue(theme: Theme): ThemeValue {
  return { tokens: theme, render: renderTheme(theme) }
}

const ThemeContext = createContext<ThemeValue>(themeValue(defaultTheme))

export function ThemeProvider(props: ParentProps & { theme?: Theme }) {
  return <ThemeContext.Provider value={themeValue(props.theme ?? defaultTheme)}>{props.children}</ThemeContext.Provider>
}

export function useTheme(): RenderTheme {
  return useContext(ThemeContext).render
}

/** The theme as written, for code that must read a token rather than paint
 * with it, such as template gradients. */
export function useThemeTokens(): Theme {
  return useContext(ThemeContext).tokens
}
