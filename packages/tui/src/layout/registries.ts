import type { PaneStyle } from "@layout/trees.ts";
import { WidgetKind } from "@layout/trees.ts";
import { ThreadViewWidget } from "@widgets/conversation/threadViews.tsx";
import type { Component } from "solid-js";

export type WidgetProps = {
  paneStyle?: PaneStyle;
  focusId: string;
};

/** The built-in, interactive widget registry. File-backed templates are
 * rendered separately because they deliberately do not accept focus props. */
export const WIDGET_REGISTRY: Record<WidgetKind, Component<WidgetProps>> = {
  [WidgetKind.Conversation]: ThreadViewWidget,
};
