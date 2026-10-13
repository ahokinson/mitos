import { expect, test } from "bun:test";
import { AppStateProvider } from "@app/states.tsx";
import { createFocusRing } from "@input/focusRings.ts";
import {
  type LayoutConfig,
  LayoutDirection,
  type LayoutNode,
} from "@layout/configs.ts";
import { LayoutRenderer, resolvedBounds } from "@layout/renderers.tsx";
import { testRender } from "@opentui/solid";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";

const state = {
  list: { threads: () => [], loading: () => false, refresh: () => {} },
  selected: () => null,
  currentHarness: () => "codex",
  events: { events: () => [], sync: () => {} },
  feedback: { lines: () => [], push: () => {} },
  onSend: () => {},
  loadHistory: () => [],
  onDraftChange: () => {},
  registerSuggestionApplier: () => {},
  suggestions: () => [],
  suggestionCursor: () => 0,
  working: () => false,
  pendingRequests: () => [],
  usage: () => null,
  mining: () => null,
  focusRing: createFocusRing(() => []),
} as unknown as Parameters<typeof AppStateProvider>[0]["value"];

test("template panels render and undeclared panel ids leave an empty pane", async () => {
  const config: LayoutConfig = {
    panels: {
      stats: { templatePath: "stats.hbs", template: "harness: {{harness}}" },
    },
    tree: {
      direction: LayoutDirection.Row,
      children: [{ panel: "stats" }, { panel: "ghost" }],
    },
  };
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <AppStateProvider value={state}>
          <LayoutRenderer config={config} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 40, height: 6 },
  );
  try {
    await setup.renderOnce();
    expect(setup.captureCharFrame()).toContain("harness: codex");
  } finally {
    setup.renderer.destroy();
  }
});

test("chart panels render their series inside the layout", async () => {
  const state = {
    list: { threads: () => [], loading: () => false, refresh: () => {} },
    selected: () => null,
    currentHarness: () => "opencode",
    events: { events: () => [], sync: () => {} },
    feedback: { lines: () => [], push: () => {} },
    onSend: () => {},
    loadHistory: () => [],
    onDraftChange: () => {},
    registerSuggestionApplier: () => {},
    suggestions: () => [],
    suggestionCursor: () => 0,
    working: () => false,
    pendingRequests: () => [],
    usage: () => null,
    mining: () => ({
      totals: {
        sessions: 1,
        messages: 1,
        messages_user: 1,
        messages_assistant: 0,
        tool_calls: 1,
        patches: 0,
        files_patched: 0,
        forks: 0,
        cost_usd: 2.0,
        tool_seconds: 0,
        tokens_input: 0,
        tokens_output: 0,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        tokens_cache_write: 0,
      },
      harnesses: [],
      cost_by_day: [],
      cost_by_project: [{ label: "/etc/nixos", value: 2.0 }],
      cost_by_model: [],
      cost_by_agent: [],
      cost_by_harness: [],
      sessions_by_day: [],
      sessions_by_agent: [],
      sessions_by_model: [],
      tokens_by_day: [],
      tool_usage: [],
      tool_time: [],
      activity_by_day: [],
      sessions: [],
    }),
    focusRing: createFocusRing(() => []),
  } as unknown as Parameters<typeof AppStateProvider>[0]["value"];
  const config: LayoutConfig = {
    panels: {
      conversation: { kind: "conversation" },
      spend: { kind: "bars", metric: "cost_by_project" },
    },
    tree: {
      direction: LayoutDirection.Row,
      children: [
        { panel: "conversation", size: "50%" },
        { panel: "spend", size: "50%" },
      ],
    },
  };
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <AppStateProvider value={state}>
          <LayoutRenderer config={config} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 90, height: 6 },
  );
  try {
    await setup.renderOnce();
    expect(setup.captureCharFrame()).toContain("/etc/nixos");
  } finally {
    setup.renderer.destroy();
  }
});

test("the conversation panel renders inside the layout", async () => {
  const config: LayoutConfig = {
    panels: { conversation: { kind: "conversation" } },
    tree: { panel: "conversation" },
  };
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <AppStateProvider value={state}>
          <LayoutRenderer config={config} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 40, height: 6 },
  );
  try {
    await setup.renderOnce();
    expect(setup.captureCharFrame()).toContain("Start a conversation.");
  } finally {
    setup.renderer.destroy();
  }
});

test("resolvedBounds splits percent and fixed claims, then flex shares", () => {
  const tree: LayoutNode = {
    direction: LayoutDirection.Row,
    children: [
      { panel: "chat", size: "60%" },
      {
        direction: LayoutDirection.Column,
        size: "40%",
        children: [
          { panel: "trend", size: 5 },
          { panel: "spend", size: 7 },
          { panel: "recent" },
        ],
      },
    ],
  };
  const bounds = resolvedBounds(tree, { width: 100, height: 40 });
  expect(bounds.get("chat")).toEqual({ width: 60, height: 40 });
  expect(bounds.get("trend")).toEqual({ width: 40, height: 5 });
  expect(bounds.get("spend")).toEqual({ width: 40, height: 7 });
  expect(bounds.get("recent")).toEqual({ width: 40, height: 28 });
});
