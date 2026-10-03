import type { MitosConfig } from "@config/loaders.ts";
import { isTemplateWidget, type LayoutNode } from "@layout/trees.ts";
import { DEFAULT_THEME_NAME } from "@theme/palettes.ts";

function quote(value: string): string {
  return JSON.stringify(value);
}

function inlineNode(node: LayoutNode): string {
  if ("widget" in node)
    return `{ widget = ${quote(node.widget)}${node.size === undefined ? "" : `, size = ${quoteOrNumber(node.size)}`} }`;
  return `{ direction = ${quote(node.direction)}${node.size === undefined ? "" : `, size = ${quoteOrNumber(node.size)}`}, children = [${node.children.map(inlineNode).join(", ")}] }`;
}

function quoteOrNumber(value: string | number): string {
  return typeof value === "number" ? String(value) : quote(value);
}

export function serializeConfig(config: MitosConfig): string {
  const sections: string[] = [];
  if (config.theme?.name || config.theme?.overrides) {
    const lines = ["[theme]"];
    if (config.theme.name) lines.push(`name = ${quote(config.theme.name)}`);
    if (config.theme.overrides) {
      lines.push("", "[theme.overrides]");
      for (const [key, value] of Object.entries(config.theme.overrides))
        if (value !== undefined) lines.push(`${key} = ${quote(value)}`);
    }
    sections.push(lines.join("\n"));
  }
  if (config.session?.defaultHarness)
    sections.push(
      `[session]\ndefault_harness = ${quote(config.session.defaultHarness)}`,
    );
  if (config.handoff?.maxInlineBytes)
    sections.push(
      `[handoff]\nmax_inline_bytes = ${config.handoff.maxInlineBytes}`,
    );
  if (config.widgets) {
    for (const [id, widget] of Object.entries(config.widgets)) {
      const lines = isTemplateWidget(widget)
        ? [`[widgets.${id}]`, `template = ${quote(widget.templatePath)}`]
        : [`[widgets.${id}]`, `kind = ${quote(widget.kind)}`];
      if (!isTemplateWidget(widget) && widget.focusKey)
        lines.push(`focus_key = ${quote(widget.focusKey)}`);
      if (isTemplateWidget(widget) && widget.align)
        lines.push(`align = ${quote(widget.align)}`);
      sections.push(lines.join("\n"));
    }
  }
  if (config.layout)
    sections.push(`[layout]\n${inlineNode(config.layout.tree).slice(2, -2)}`);
  if (config.keybindings && Object.keys(config.keybindings).length > 0) {
    const lines = ["[keybindings]"];
    for (const [action, keys] of Object.entries(config.keybindings))
      lines.push(`${action} = [${keys.map(quote).join(", ")}]`);
    sections.push(lines.join("\n"));
  }
  return sections.length > 0 ? `${sections.join("\n\n")}\n` : "";
}

export const DEFAULT_CONFIG_TEMPLATE = `# mitos personal configuration. Safe to edit — delete this file to return to the default interface.

# [session]
# default_harness = "codex"

[theme]
name = "${DEFAULT_THEME_NAME}"
`;
