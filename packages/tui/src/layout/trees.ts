export enum WidgetKind {
  Conversation = "conversation",
}

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

/** A placement references a named declaration in `[widgets.<id>]`. */
export type LayoutNode =
  | { widget: string; size?: PaneSize }
  | { direction: LayoutDirection; size?: PaneSize; children: LayoutNode[] };

export type ConversationWidgetConfig = {
  kind: WidgetKind;
  focusKey?: string;
};

/** A loaded, read-only Handlebars view. `templatePath` is retained so a
 * configuration can be written back without embedding its source. */
export type TemplateWidgetConfig = {
  templatePath: string;
  template: string;
  align?: TextAlign;
};

export type WidgetConfig = ConversationWidgetConfig | TemplateWidgetConfig;

/** The configuration shape before template files have been read. */
export type TemplateWidgetDeclaration = {
  templatePath: string;
  align?: TextAlign;
};

export type WidgetDeclaration =
  | ConversationWidgetConfig
  | TemplateWidgetDeclaration;

export type LayoutConfig = {
  tree: LayoutNode;
  widgets: Record<string, WidgetConfig>;
};

export type LayoutDeclaration = {
  tree: LayoutNode;
  widgets: Record<string, WidgetDeclaration>;
};

export type PaneStyle = {
  width?: PaneSize;
  height?: PaneSize;
  flexGrow?: number;
};

export const DEFAULT_WIDGETS: Record<string, WidgetConfig> = {
  conversation: { kind: WidgetKind.Conversation, focusKey: "ctrl+1" },
};

export const DEFAULT_LAYOUT: LayoutNode = { widget: "conversation" };

export const DEFAULT_LAYOUT_CONFIG: LayoutConfig = {
  tree: DEFAULT_LAYOUT,
  widgets: DEFAULT_WIDGETS,
};

const TEXT_ALIGNS = Object.values(TextAlign) as string[];
const LAYOUT_DIRECTIONS = Object.values(LayoutDirection) as string[];
const PERCENT_SIZE_RE = /^\d+%$/;
const WIDGET_ID_RE = /^[A-Za-z][A-Za-z0-9_-]*$/;

function isPaneSize(value: unknown): value is PaneSize {
  return (
    value === undefined ||
    typeof value === "number" ||
    (typeof value === "string" && PERCENT_SIZE_RE.test(value))
  );
}

function parseWidgetDeclaration(value: unknown): WidgetDeclaration | null {
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;
  if (record.kind === WidgetKind.Conversation) {
    if (record.template !== undefined || record.align !== undefined)
      return null;
    if (record.focus_key !== undefined && typeof record.focus_key !== "string")
      return null;
    return record.focus_key === undefined
      ? { kind: WidgetKind.Conversation }
      : { kind: WidgetKind.Conversation, focusKey: record.focus_key };
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

export function isLayoutNode(value: unknown): value is LayoutNode {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  if ("widget" in record)
    return typeof record.widget === "string" && isPaneSize(record.size);
  return (
    typeof record.direction === "string" &&
    LAYOUT_DIRECTIONS.includes(record.direction) &&
    Array.isArray(record.children) &&
    record.children.every(isLayoutNode) &&
    isPaneSize(record.size)
  );
}

export function cleanLayoutNode(node: LayoutNode): LayoutNode {
  if ("widget" in node)
    return node.size === undefined
      ? { widget: node.widget }
      : { widget: node.widget, size: node.size };
  const children = node.children.map(cleanLayoutNode);
  return node.size === undefined
    ? { direction: node.direction, children }
    : { direction: node.direction, size: node.size, children };
}

/** Widget IDs are stable across layout rearrangement and act as focus IDs. */
export function collectLeafIds(node: LayoutNode): string[] {
  if ("widget" in node) return [node.widget];
  return node.children.flatMap(collectLeafIds);
}

export function isConversationWidget(
  widget: WidgetConfig | WidgetDeclaration,
): widget is ConversationWidgetConfig {
  return "kind" in widget && widget.kind === WidgetKind.Conversation;
}

export function isTemplateWidget(
  widget: WidgetConfig,
): widget is TemplateWidgetConfig {
  return "template" in widget;
}

/** Template views are read-only. Only conversation panes join the focus ring. */
export function collectFocusableIds(config: LayoutConfig): string[] {
  return collectLeafIds(config.tree).filter((id) => {
    const widget = config.widgets[id];
    return widget !== undefined && isConversationWidget(widget);
  });
}

export function validateLayoutConfig(
  rawWidgets: unknown,
  rawLayout: unknown,
): LayoutDeclaration | null {
  if (
    typeof rawWidgets !== "object" ||
    rawWidgets === null ||
    Array.isArray(rawWidgets) ||
    !isLayoutNode(rawLayout)
  )
    return null;
  const widgets: Record<string, WidgetDeclaration> = {};
  for (const [id, rawWidget] of Object.entries(
    rawWidgets as Record<string, unknown>,
  )) {
    const widget = parseWidgetDeclaration(rawWidget);
    if (!WIDGET_ID_RE.test(id) || !widget) return null;
    widgets[id] = widget;
  }
  const tree = cleanLayoutNode(rawLayout);
  const leaves = collectLeafIds(tree);
  if (
    leaves.length === 0 ||
    new Set(leaves).size !== leaves.length ||
    leaves.some((id) => widgets[id] === undefined)
  )
    return null;
  const focusKeys = Object.values(widgets).flatMap((widget) =>
    isConversationWidget(widget) && widget.focusKey ? [widget.focusKey] : [],
  );
  if (new Set(focusKeys).size !== focusKeys.length) return null;
  return { tree, widgets };
}
