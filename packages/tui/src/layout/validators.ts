import {
  CHART_KINDS,
  CHART_METRICS,
  ChartKind,
  type ChartMetric,
  HEATMAP_METRIC,
  isConversationPanel,
  type LayoutDeclaration,
  LayoutDirection,
  type LayoutNode,
  type PanelDeclaration,
  PanelKind,
  type PaneSize,
  TextAlign,
} from "@layout/configs.ts";
import {
  cleanLayoutNode,
  collectLeafIds,
  countTemplatePanels,
} from "@layout/trees.ts";

const TEXT_ALIGNS = Object.values(TextAlign) as string[];
const LAYOUT_DIRECTIONS = Object.values(LayoutDirection) as string[];
const PERCENT_SIZE_RE = /^\d+%$/;
const PANEL_ID_RE = /^[A-Za-z][A-Za-z0-9_-]*$/;

function isPaneSize(value: unknown): value is PaneSize {
  return (
    value === undefined ||
    typeof value === "number" ||
    (typeof value === "string" && PERCENT_SIZE_RE.test(value))
  );
}

function parsePanelDeclaration(value: unknown): PanelDeclaration | null {
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;
  if (record.kind === PanelKind.Conversation) {
    if (record.template !== undefined || record.align !== undefined)
      return null;
    if (record.focus_key !== undefined && typeof record.focus_key !== "string")
      return null;
    return record.focus_key === undefined
      ? { kind: PanelKind.Conversation }
      : { kind: PanelKind.Conversation, focusKey: record.focus_key };
  }
  if (typeof record.kind === "string" && CHART_KINDS.includes(record.kind)) {
    if (
      record.template !== undefined ||
      record.align !== undefined ||
      record.focus_key !== undefined
    )
      return null;
    const kind = record.kind as ChartKind;
    if (record.top !== undefined && !isTopSize(record.top)) return null;
    if (kind === ChartKind.Sessions) {
      if (record.metric !== undefined) return null;
      return {
        kind,
        ...(typeof record.top === "number" && { top: record.top }),
      };
    }
    if (kind === ChartKind.Usage) {
      if (record.metric !== undefined || record.top !== undefined) return null;
      return { kind };
    }
    const metric = chartMetric(kind, record.metric);
    if (metric === undefined) return null;
    return {
      kind,
      metric,
      ...(typeof record.top === "number" && { top: record.top }),
    };
  }
  if (
    record.kind === undefined &&
    record.focus_key === undefined &&
    typeof record.template === "string" &&
    record.template.length > 0
  ) {
    if (record.align === undefined) return { templatePath: record.template };
    if (!TEXT_ALIGNS.includes(record.align as string)) return null;
    return { templatePath: record.template, align: record.align as TextAlign };
  }
  return null;
}

/** A chart's `metric` names the mined series it draws: the heatmap takes the
 * activity day rows only, `bars` and `spark` take a grouped series. */
function chartMetric(kind: ChartKind, metric: unknown): string | undefined {
  if (typeof metric !== "string" || metric.length === 0) return undefined;
  if (kind === ChartKind.Heatmap)
    return metric === HEATMAP_METRIC ? metric : undefined;
  return CHART_METRICS.includes(metric as ChartMetric) ? metric : undefined;
}

function isTopSize(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 1;
}

export function isLayoutNode(value: unknown): value is LayoutNode {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  if ("panel" in record)
    return typeof record.panel === "string" && isPaneSize(record.size);
  return (
    typeof record.direction === "string" &&
    LAYOUT_DIRECTIONS.includes(record.direction) &&
    Array.isArray(record.children) &&
    record.children.every(isLayoutNode) &&
    isPaneSize(record.size)
  );
}

export function validateLayoutConfig(
  rawPanels: unknown,
  rawLayout: unknown,
): LayoutDeclaration | null {
  if (
    typeof rawPanels !== "object" ||
    rawPanels === null ||
    Array.isArray(rawPanels) ||
    !isLayoutNode(rawLayout)
  )
    return null;
  const panels: Record<string, PanelDeclaration> = {};
  for (const [id, rawPanel] of Object.entries(
    rawPanels as Record<string, unknown>,
  )) {
    const panel = parsePanelDeclaration(rawPanel);
    if (!PANEL_ID_RE.test(id) || !panel) return null;
    panels[id] = panel;
  }
  if (countTemplatePanels(panels) > 1) return null;
  const tree = cleanLayoutNode(rawLayout);
  const leaves = collectLeafIds(tree);
  if (
    leaves.length === 0 ||
    new Set(leaves).size !== leaves.length ||
    leaves.some((id) => panels[id] === undefined)
  )
    return null;
  const focusKeys = Object.values(panels).flatMap((panel) =>
    isConversationPanel(panel) && panel.focusKey ? [panel.focusKey] : [],
  );
  if (new Set(focusKeys).size !== focusKeys.length) return null;
  return { tree, panels };
}
