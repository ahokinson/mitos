import { expect, test } from "bun:test";

import {
  DEFAULT_LAYOUT_CONFIG,
  LayoutDirection,
  PanelKind,
  TextAlign,
} from "@layout/configs.ts";
import { collectFocusableIds, collectLeafIds } from "@layout/trees.ts";
import { validateLayoutConfig } from "@layout/validators.ts";

test("default layout is one named conversation panel", () => {
  expect(DEFAULT_LAYOUT_CONFIG).toEqual({
    tree: { panel: "conversation" },
    panels: {
      conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
    },
  });
});

test("named split layout preserves stable panel identities", () => {
  const layout = validateLayoutConfig(
    {
      conversation: { kind: "conversation", focus_key: "ctrl+1" },
      stats: { template: "panels/stats.hbs" },
    },
    {
      direction: LayoutDirection.Row,
      children: [{ panel: "conversation" }, { panel: "stats", size: 30 }],
    },
  );
  if (!layout) throw new Error("expected a valid layout");
  expect(collectLeafIds(layout.tree)).toEqual(["conversation", "stats"]);
});

test("a template view declaration carries its path through", () => {
  const templatePath = "panels/stats.hbs";
  const layout = validateLayoutConfig(
    {
      conversation: { kind: "conversation" },
      stats: { template: templatePath },
    },
    {
      direction: "row",
      children: [{ panel: "conversation" }, { panel: "stats", size: "25%" }],
    },
  );
  expect(layout?.panels.stats).toEqual({ templatePath });
});

test("a template view needs a nonempty path and cannot declare kind or focus", () => {
  expect(validateLayoutConfig({ stats: {} }, { panel: "stats" })).toBeNull();
  expect(
    validateLayoutConfig({ stats: { template: "" } }, { panel: "stats" }),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { stats: { kind: "custom", template: "panels/stats.hbs" } },
      { panel: "stats" },
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { stats: { template: "panels/stats.hbs", focus_key: "ctrl+2" } },
      { panel: "stats" },
    ),
  ).toBeNull();
});

test("a template view may declare a valid align, and nothing else does", () => {
  const layout = { panel: "stats" };
  expect(
    validateLayoutConfig(
      { stats: { template: "panels/stats.hbs", align: "center" } },
      layout,
    )?.panels.stats,
  ).toEqual({ templatePath: "panels/stats.hbs", align: TextAlign.Center });
  expect(
    validateLayoutConfig(
      { stats: { template: "panels/stats.hbs", align: "middle" } },
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
      children: [{ panel: "conversation" }, { panel: "stats" }],
    },
    panels: {
      conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
      stats: { templatePath: "panels/stats.hbs", template: "{{harness}}" },
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
      { direction: "row", children: [{ panel: "left" }, { panel: "right" }] },
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      { conversation: { kind: "conversation" } },
      {
        direction: "row",
        children: [{ panel: "conversation" }, { panel: "conversation" }],
      },
    ),
  ).toBeNull();
});

test("chart declarations carry their kind, metric and top through", () => {
  const layout = validateLayoutConfig(
    {
      conversation: { kind: "conversation" },
      spend: { kind: "bars", metric: "cost_by_project", top: 8 },
      trend: { kind: "spark", metric: "cost_by_day" },
      activity: { kind: "heatmap", metric: "activity_by_day" },
      recent: { kind: "sessions", top: 5 },
      live: { kind: "usage" },
    },
    {
      direction: "row",
      children: [
        { panel: "conversation" },
        { panel: "spend" },
        { panel: "trend" },
        { panel: "activity" },
        { panel: "recent" },
        { panel: "live" },
      ],
    },
  );
  expect(layout?.panels.spend).toEqual({
    kind: "bars",
    metric: "cost_by_project",
    top: 8,
  });
  expect(layout?.panels.trend).toEqual({
    kind: "spark",
    metric: "cost_by_day",
  });
  expect(layout?.panels.activity).toEqual({
    kind: "heatmap",
    metric: "activity_by_day",
  });
  expect(layout?.panels.recent).toEqual({ kind: "sessions", top: 5 });
  expect(layout?.panels.live).toEqual({ kind: "usage" });
});

test("a usage chart takes neither a metric nor a top", () => {
  const tree = {
    direction: "row",
    children: [{ panel: "conversation" }, { panel: "chart" as const }],
  };
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "usage", metric: "cost_by_day" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "usage", top: 5 },
      },
      tree,
    ),
  ).toBeNull();
});

test("a bars or spark chart needs a known series metric", () => {
  const tree = {
    direction: "row",
    children: [{ panel: "conversation" }, { panel: "chart" as const }],
  };
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "no_such_series" },
      },
      tree,
    ),
  ).toBeNull();
});

test("a heatmap takes the activity series only and sessions takes none", () => {
  const tree = {
    direction: "row",
    children: [{ panel: "conversation" }, { panel: "chart" as const }],
  };
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "heatmap", metric: "cost_by_project" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "sessions", metric: "cost_by_project" },
      },
      tree,
    ),
  ).toBeNull();
});

test("chart declarations reject template, align, focus and bad tops", () => {
  const tree = {
    direction: "row",
    children: [{ panel: "conversation" }, { panel: "chart" as const }],
  };
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "cost_by_day", template: "x.hbs" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "cost_by_day", align: "center" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "cost_by_day", focus_key: "ctrl+2" },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "cost_by_day", top: 0 },
      },
      tree,
    ),
  ).toBeNull();
  expect(
    validateLayoutConfig(
      {
        conversation: { kind: "conversation" },
        chart: { kind: "bars", metric: "cost_by_day", top: 1.5 },
      },
      tree,
    ),
  ).toBeNull();
});

test("chart views are omitted from the focus ring", () => {
  const config = {
    tree: {
      direction: LayoutDirection.Row,
      children: [{ panel: "conversation" }, { panel: "spend" }],
    },
    panels: {
      conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
      spend: { kind: "bars", metric: "cost_by_project" },
    },
  };
  expect(collectFocusableIds(config)).toEqual(["conversation"]);
});
