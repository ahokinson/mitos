import { resolveThemePath } from "@config/paths.ts";
import { importToml, warn } from "@config/tomls.ts";
import type { Theme, ThemePathsConfig } from "@theme/themes.ts";
import { isTheme } from "@theme/validators.ts";

export async function loadCustomThemes(
  paths: ThemePathsConfig,
  configPath: string,
): Promise<Record<string, Theme>> {
  const result: Record<string, Theme> = {};
  for (const [name, rawPath] of Object.entries(paths)) {
    try {
      const raw = await importToml(resolveThemePath(rawPath, configPath));
      if (!isTheme(raw)) throw new Error("is missing required tokens");
      result[name] = raw;
    } catch (cause) {
      warn(
        `custom theme "${name}" could not be read (${(cause as Error).message}) — skipping`,
      );
    }
  }
  return result;
}
