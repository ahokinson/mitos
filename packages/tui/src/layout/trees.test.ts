import { expect, test } from "bun:test";

import {
  collectFocusableIds,
  collectLeafIds,
  DEFAULT_LAYOUT_CONFIG,
  LayoutDirection,
  TextAlign,
  validateLayoutConfig,
  WidgetKind,
} from "@layout/trees.ts";

test("default layout is one named conversation widget", () => {
  expect(DEFAULT_LAYOUT_CONFIG).toEqual({
    tree: { widget: "conversation" },
    widgets: {
      conversation: { kind: WidgetKind.Conversation, focusKey: "ctrl+1" },
    },
  });
});

test("named split layout preserves stable widget identities", () => {
  const layout = validateLayoutConfig(
    {
      conversation: { kind: "conversation", focus_key: "ctrl+1" },
      stats: { template: "widgets/stats.hbs" },
    },
    {
      direction: LayoutDirection.Row,
      children: [{ widget: "conversation" }, { widget: "stats", size: 30 }],
    },
  );
  if (!layout) throw new Error("expected a valid layout");
  expect(collectLeafIds(layout.tree)).toEqual(["conversation", "stats"]);
});

test("a template view declaration carries its path through", () => {
  const templatePath = "widgets/stats.hbs";
  const layout = validateLayoutConfig(
    {
      conversation: { kind: "conversation" },
      stats: { template: templatePath },
    },
    {
      direction: "row",
      children: [{ widget: "conversation" }, { widget: "stats", size: "25%" }],
    },
  );
  expect(layout?.widgets.stats).toEqual({ templatePath });
});

test("a template view needs a nonempty path and cannot declare kind or focus", () => {
  expect(validateLayoutConfig({ stats: {} }, { widget: "stats" })).toBeNull();
  expect(
    validateLayoutConfig({ stats: { template: "" } }, { widget: "stats" }),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { stats: { kind: "custom", template: "widgets/stats.hbs" } },
      { widget: "stats" },
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { stats: { template: "widgets/stats.hbs", focus_key: "ctrl+2" } },
      { widget: "stats" },
    ),
  ).toBeNull();
});

test("a template view may declare a valid align, and nothing else does", () => {
  const layout = { widget: "stats" };
  expect(
    validateLayoutConfig(
      { stats: { template: "widgets/stats.hbs", align: "center" } },
      layout,
    )?.widgets.stats,
  ).toEqual({ templatePath: "widgets/stats.hbs", align: TextAlign.Center });
  expect(
    validateLayoutConfig(
      { stats: { template: "widgets/stats.hbs", align: "middle" } },
      layout,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { stats: { kind: "conversation", align: "left" } },
      layout,
    ),
  ).toBeNull();
});

test("template views are omitted from the focus ring", () => {
  const config = {
    tree: {
      direction: LayoutDirection.Row,
      children: [{ widget: "conversation" }, { widget: "stats" }],
    },
    widgets: {
      conversation: { kind: WidgetKind.Conversation, focusKey: "ctrl+1" },
      stats: { templatePath: "widgets/stats.hbs", template: "{{harness}}" },
    },
  };
  expect(collectFocusableIds(config)).toEqual(["conversation"]);
});

test("layout rejects duplicate placements and focus shortcuts", () => {
  expect(
    validateLayoutConfig(
      {
        left: { kind: "conversation", focus_key: "ctrl+1" },
        right: { kind: "conversation", focus_key: "ctrl+1" },
      },
      { direction: "row", children: [{ widget: "left" }, { widget: "right" }] },
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { conversation: { kind: "conversation" } },
      {
        direction: "row",
        children: [{ widget: "conversation" }, { widget: "conversation" }],
      },
    ),
  ).toBeNull();
});
