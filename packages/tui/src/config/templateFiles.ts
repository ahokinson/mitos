import { readFile } from "node:fs/promises";

import { resolveTemplatePath } from "@config/paths.ts";
import { warn } from "@config/tomls.ts";
import type {
  LayoutConfig,
  LayoutDeclaration,
  WidgetConfig,
} from "@layout/trees.ts";
import { isConversationWidget } from "@layout/trees.ts";

/** Reads each declared template once at startup. A template's configured path
 * stays in the resolved configuration so serialization never embeds source. */
export async function loadTemplateViews(
  declaration: LayoutDeclaration,
  configPath: string,
): Promise<LayoutConfig | null> {
  const widgets: Record<string, WidgetConfig> = {};
  for (const [id, widget] of Object.entries(declaration.widgets)) {
    if (isConversationWidget(widget)) {
      widgets[id] = widget;
      continue;
    }
    const path = resolveTemplatePath(widget.templatePath, configPath);
    try {
      const template = await readFile(path, "utf8");
      if (template.length === 0) throw new Error("file is empty");
      widgets[id] = {
        templatePath: widget.templatePath,
        template,
        ...(widget.align && { align: widget.align }),
      };
    } catch (cause) {
      warn(
        `template view "${id}" could not be read from ${path} (${(cause as Error).message})`,
      );
      return null;
    }
  }
  return { tree: declaration.tree, widgets };
}
