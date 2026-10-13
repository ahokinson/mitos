import {
  isChartPanel,
  isTemplatePanel,
  type LayoutConfig,
  LayoutDirection,
  type LayoutNode,
  type PaneSize,
} from "@layout/configs.ts";
import { CHART_REGISTRY, PANEL_REGISTRY } from "@layout/registries.ts";
import { renderTile, type TileNode } from "@layout/tiles.tsx";
import { useTerminalDimensions } from "@opentui/solid";
import { TemplatePanel } from "@panels/custom/customPanels.tsx";
import type { JSX } from "solid-js";
import { createMemo } from "solid-js";

/** A panel pane's resolved size in characters, measured the way the
 * layout engine splits: percent and fixed sizes claim their share first,
 * then the unsized children split what is left. */
export type PaneBounds = { width: number; height: number };

function claimedShare(size: PaneSize, total: number): number {
  if (typeof size === "number") return size;
  return (total * Number.parseInt(size, 10)) / 100;
}

export function resolvedBounds(
  node: LayoutNode,
  pane: PaneBounds,
): Map<string, PaneBounds> {
  const result = new Map<string, PaneBounds>();
  const visit = (node: LayoutNode, pane: PaneBounds) => {
    if ("panel" in node) {
      result.set(node.panel, pane);
      return;
    }
    const row = node.direction === LayoutDirection.Row;
    const total = row ? pane.width : pane.height;
    let claimed = 0;
    let flexing = 0;
    for (const child of node.children) {
      if (child.size === undefined) flexing += 1;
      else claimed += claimedShare(child.size, total);
    }
    const flexShare =
      flexing > 0 ? Math.max(0, (total - claimed) / flexing) : 0;
    for (const child of node.children) {
      const share =
        child.size === undefined ? flexShare : claimedShare(child.size, total);
      const childPane = row
        ? { width: share, height: pane.height }
        : { width: pane.width, height: share };
      visit(child, childPane);
    }
  };
  visit(node, pane);
  return result;
}

function toTileNode(node: LayoutNode): TileNode<string> {
  return "panel" in node
    ? { leaf: node.panel, size: node.size }
    : {
        direction: node.direction,
        size: node.size,
        children: node.children.map(toTileNode),
      };
}

export function LayoutRenderer(props: { config: LayoutConfig }): JSX.Element {
  const dimensions = useTerminalDimensions();
  const bounds = createMemo(() =>
    resolvedBounds(props.config.tree, {
      width: dimensions().width,
      height: dimensions().height,
    }),
  );
  return renderTile(
    toTileNode(props.config.tree),
    (panelId, style) => {
      const definition = props.config.panels[panelId];
      if (!definition) return <box {...style} />;
      if (isTemplatePanel(definition))
        return (
          <TemplatePanel
            paneStyle={style}
            template={definition.template}
            align={definition.align}
          />
        );
      if (isChartPanel(definition)) {
        const Chart = CHART_REGISTRY[definition.kind];
        const pane = bounds().get(panelId);
        return (
          <Chart
            paneStyle={style}
            paneWidth={pane?.width}
            metric={definition.metric}
            top={definition.top}
          />
        );
      }
      const Panel = PANEL_REGISTRY[definition.kind];
      return <Panel paneStyle={style} focusId={panelId} />;
    },
    { flexGrow: 1 },
  );
}
