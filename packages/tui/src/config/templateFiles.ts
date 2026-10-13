import { readFile } from "node:fs/promises";

import { resolveTemplatePath } from "@config/paths.ts";
import { warn } from "@config/tomls.ts";
import {
  isChartPanel,
  isConversationPanel,
  type LayoutConfig,
  type LayoutDeclaration,
  type PanelConfig,
} from "@layout/configs.ts";

/** Reads the layout's one template file at startup; the validator has
 * already rejected a layout that declares more than one. A template's
 * configured path stays in the resolved configuration so serialization
 * never embeds source. */
export async function loadTemplateViews(
  declaration: LayoutDeclaration,
  configPath: string,
): Promise<LayoutConfig | null> {
  const panels: Record<string, PanelConfig> = {};
  for (const [id, panel] of Object.entries(declaration.panels)) {
    if (isConversationPanel(panel) || isChartPanel(panel)) {
      panels[id] = panel;
      continue;
    }
    const path = resolveTemplatePath(panel.templatePath, configPath);
    try {
      const template = await readFile(path, "utf8");
      if (template.length === 0) throw new Error("file is empty");
      panels[id] = {
        templatePath: panel.templatePath,
        template,
        ...(panel.align && { align: panel.align }),
      };
    } catch (cause) {
      warn(
        `template view "${id}" could not be read from ${path} (${(cause as Error).message})`,
      );
      return null;
    }
  }
  return { tree: declaration.tree, panels };
}
