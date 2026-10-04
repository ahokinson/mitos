import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { AppStateProvider } from "@app/states.tsx"
import { createFocusRing } from "@input/focusRings.ts"
import { LayoutRenderer } from "@layout/renderers.tsx"
import { LayoutDirection, type LayoutConfig } from "@layout/trees.ts"
import { resolveTheme } from "@theme/palettes.ts"
import { ThemeProvider } from "@theme/providers.tsx"

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
  focusRing: createFocusRing(() => []),
} as unknown as Parameters<typeof AppStateProvider>[0]["value"]

test("template widgets render and undeclared widget ids leave an empty pane", async () => {
  const config: LayoutConfig = {
    widgets: { stats: { templatePath: "stats.hbs", template: "harness: {{harness}}" } },
    tree: {
      direction: LayoutDirection.Row,
      children: [{ widget: "stats" }, { widget: "ghost" }],
    },
  }
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <AppStateProvider value={state}>
          <LayoutRenderer config={config} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 40, height: 6 },
  )
  try {
    await setup.renderOnce()
    expect(setup.captureCharFrame()).toContain("harness: codex")
  } finally {
    setup.renderer.destroy()
  }
})
