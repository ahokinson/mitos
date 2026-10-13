import {
  isConversationPanel,
  type LayoutConfig,
  type LayoutNode,
  type PanelDeclaration,
} from "@layout/configs.ts";

export function cleanLayoutNode(node: LayoutNode): LayoutNode {
  if ("panel" in node)
    return node.size === undefined
      ? { panel: node.panel }
      : { panel: node.panel, size: node.size };
  const children = node.children.map(cleanLayoutNode);
  return node.size === undefined
    ? { direction: node.direction, children }
    : { direction: node.direction, size: node.size, children };
}

/** Panel IDs are stable across layout rearrangement and act as focus IDs. */
export function collectLeafIds(node: LayoutNode): string[] {
  if ("panel" in node) return [node.panel];
  return node.children.flatMap(collectLeafIds);
}

/** Template views are read-only. Only conversation panes join the focus ring. */
export function collectFocusableIds(config: LayoutConfig): string[] {
  return collectLeafIds(config.tree).filter((id) => {
    const panel = config.panels[id];
    return panel !== undefined && isConversationPanel(panel);
  });
}

/** A layout holds at most one template panel: its file is the whole custom
 * view, read once, so every panel a person draws lives in one place. */
export function countTemplatePanels(
  panels: Record<string, PanelDeclaration>,
): number {
  return Object.values(panels).filter((panel) => "templatePath" in panel)
    .length;
}
