export enum PanelKind {
  Conversation = "conversation",
}

/** The chart kinds a `[panels.<id>]` declaration can ask for; each renders
 * one mined series as a read-only view, except `usage`, which renders the
 * active harness's live rate windows. */
export enum ChartKind {
  Bars = "bars",
  Spark = "spark",
  Heatmap = "heatmap",
  Sessions = "sessions",
  Usage = "usage",
}

export const CHART_KINDS = Object.values(ChartKind) as string[];

/** The grouped series a `bars` or `spark` chart can draw. The `heatmap`
 * chart takes the activity day rows only. */
export const CHART_METRICS = [
  "cost_by_day",
  "cost_by_project",
  "cost_by_model",
  "cost_by_agent",
  "cost_by_harness",
  "sessions_by_day",
  "sessions_by_agent",
  "sessions_by_model",
  "tokens_by_day",
  "tool_usage",
  "tool_time",
] as const;

export type ChartMetric = (typeof CHART_METRICS)[number];

export const HEATMAP_METRIC = "activity_by_day";

export enum LayoutDirection {
  Row = "row",
  Column = "column",
}

export enum TextAlign {
  Left = "left",
  Center = "center",
  Right = "right",
}

export type PaneSize = number | `${number}%`;

/** A placement references a named declaration in `[panels.<id>]`. */
export type LayoutNode =
  | { panel: string; size?: PaneSize }
  | { direction: LayoutDirection; size?: PaneSize; children: LayoutNode[] };

export type ConversationPanelConfig = {
  kind: PanelKind;
  focusKey?: string;
};

/** A read-only chart over one mined OpenCode series. `metric` names the
 * series for `bars` and `spark` and is fixed for `heatmap`; `top` trims the
 * rows drawn. */
export type ChartPanelConfig = {
  kind: ChartKind;
  metric?: string;
  top?: number;
};

/** A loaded, read-only Handlebars view. `templatePath` is retained so a
 * configuration can be written back without embedding its source. */
export type TemplatePanelConfig = {
  templatePath: string;
  template: string;
  align?: TextAlign;
};

export type PanelConfig =
  | ConversationPanelConfig
  | TemplatePanelConfig
  | ChartPanelConfig;

/** The configuration shape before template files have been read. */
export type TemplatePanelDeclaration = {
  templatePath: string;
  align?: TextAlign;
};

export type PanelDeclaration =
  | ConversationPanelConfig
  | TemplatePanelDeclaration
  | ChartPanelConfig;

export type LayoutConfig = {
  tree: LayoutNode;
  panels: Record<string, PanelConfig>;
};

export type LayoutDeclaration = {
  tree: LayoutNode;
  panels: Record<string, PanelDeclaration>;
};

export type PaneStyle = {
  width?: PaneSize;
  height?: PaneSize;
  flexGrow?: number;
};

export const DEFAULT_PANELS: Record<string, PanelConfig> = {
  conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
};

export const DEFAULT_LAYOUT: LayoutNode = { panel: "conversation" };

export const DEFAULT_LAYOUT_CONFIG: LayoutConfig = {
  tree: DEFAULT_LAYOUT,
  panels: DEFAULT_PANELS,
};

export function isConversationPanel(
  panel: PanelConfig | PanelDeclaration,
): panel is ConversationPanelConfig {
  return "kind" in panel && panel.kind === PanelKind.Conversation;
}

export function isTemplatePanel(
  panel: PanelConfig,
): panel is TemplatePanelConfig {
  return "template" in panel;
}

export function isChartPanel(
  panel: PanelConfig | PanelDeclaration,
): panel is ChartPanelConfig {
  return "kind" in panel && CHART_KINDS.includes(panel.kind as string);
}
