import { existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { resolveConfigPath } from "@config/paths.ts";
import { loadTemplateViews } from "@config/templateFiles.ts";
import { loadCustomThemes } from "@config/themeFiles.ts";
import { importToml, table, warn } from "@config/tomls.ts";
import {
  type HandoffConfig,
  type SessionConfig,
  validateHandoff,
  validateKeybindings,
  validateSession,
  validateThemeConfig,
  validateThemePaths,
} from "@config/validators.ts";
import { DEFAULT_CONFIG_TEMPLATE } from "@config/writers.ts";
import type { KeybindingsConfig } from "@input/keybindings.ts";
import {
  DEFAULT_LAYOUT_CONFIG,
  type LayoutConfig,
  validateLayoutConfig,
} from "@layout/trees.ts";
import { BUILTIN_THEME_NAMES } from "@theme/palettes.ts";
import type { Theme, ThemeConfig, ThemePathsConfig } from "@theme/themes.ts";
import { createSignal } from "solid-js";

export type MitosConfig = {
  theme?: ThemeConfig;
  themes?: ThemePathsConfig;
  session?: SessionConfig;
  handoff?: HandoffConfig;
  widgets?: LayoutConfig["widgets"];
  layout?: LayoutConfig;
  keybindings?: KeybindingsConfig;
};

export type ConfigHandle = {
  config: () => MitosConfig;
  setConfig: (updater: (current: MitosConfig) => MitosConfig) => void;
  customThemes: () => Record<string, Theme>;
  configPath: string;
};

async function scaffoldDefaultConfig(configPath: string): Promise<void> {
  try {
    mkdirSync(dirname(configPath), { recursive: true });
    await Bun.write(configPath, DEFAULT_CONFIG_TEMPLATE);
  } catch (cause) {
    warn(
      `could not create ${configPath} (${(cause as Error).message}) — running on defaults for this session`,
    );
  }
}

async function readConfigTable(
  configPath: string,
): Promise<Record<string, unknown>> {
  if (!existsSync(configPath)) await scaffoldDefaultConfig(configPath);
  if (!existsSync(configPath)) return {};
  try {
    const parsedTable = table(await importToml(configPath));
    if (parsedTable) return parsedTable;
    warn(`${configPath} did not parse to a table — using defaults`);
  } catch (cause) {
    warn(
      `${configPath} could not be parsed (${(cause as Error).message}) — using defaults`,
    );
  }
  return {};
}

export async function loadConfig(overrideDir?: string): Promise<ConfigHandle> {
  const configPath = resolveConfigPath(overrideDir);
  const raw = await readConfigTable(configPath);

  const themes = validateThemePaths(raw.themes);
  const customThemesMap = await loadCustomThemes(themes, configPath);
  const theme = validateThemeConfig(raw.theme);
  if (
    theme.name &&
    !BUILTIN_THEME_NAMES.includes(theme.name) &&
    !(theme.name in customThemesMap)
  ) {
    warn(`theme "${theme.name}" is not declared — using default theme`);
    theme.name = undefined;
  }
  const layoutDeclaration =
    raw.layout === undefined && raw.widgets === undefined
      ? DEFAULT_LAYOUT_CONFIG
      : validateLayoutConfig(raw.widgets, raw.layout);
  const resolvedLayout = layoutDeclaration
    ? await loadTemplateViews(layoutDeclaration, configPath)
    : null;
  if (!resolvedLayout)
    warn(
      "[widgets] and [layout] must form a valid named layout — using default",
    );

  const [config, setConfig] = createSignal<MitosConfig>({
    theme,
    themes,
    session: validateSession(raw.session),
    handoff: validateHandoff(raw.handoff),
    widgets: (resolvedLayout ?? DEFAULT_LAYOUT_CONFIG).widgets,
    layout: resolvedLayout ?? DEFAULT_LAYOUT_CONFIG,
    keybindings: validateKeybindings(raw.keybindings),
  });
  const [customThemes] = createSignal(customThemesMap);
  return { config, setConfig, customThemes, configPath };
}
