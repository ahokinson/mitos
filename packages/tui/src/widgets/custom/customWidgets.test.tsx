import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import { RGBA } from "@opentui/core"

import { readFileSync } from "node:fs"
import { join } from "node:path"

import type { UsageSnapshot } from "@database/databases.ts"
import { TextAlign } from "@layout/trees.ts"
import { TemplateWidget } from "@widgets/custom/customWidgets.tsx"
import { AppStateProvider } from "@app/states.tsx"
import { ThreadMode, ThreadStatus, type Thread } from "@session/threads.ts"
import { ThemeProvider } from "@theme/providers.tsx"
import { resolveTheme } from "@theme/palettes.ts"
import { createFocusRing } from "@input/focusRings.ts"

const activeThread: Thread = {
  id: "a1b2c3d4-5678-90ab-cdef-1234567890ab",
  workspace_id: "ws1",
  status: ThreadStatus.Active,
  mode: ThreadMode.Build,
  active_harness: "codex",
  native_session: null,
  last_event_seq: 5,
  created_at: "2026-10-01T08:48:08.520280533+00:00",
  updated_at: "2026-10-01T08:48:17.819925381+00:00",
  opening_message: null,
}

function fakeAppState(usage: UsageSnapshot | null = null) {
  return {
    list: { threads: () => [], loading: () => false, refresh: () => {} },
    selected: () => activeThread,
    currentHarness: () => "codex",
    events: { events: () => [], sync: () => {} },
    feedback: { lines: () => [], push: () => {} },
    onSend: () => {},
    onDraftChange: () => {},
    registerSuggestionApplier: () => {},
    suggestions: () => [],
    suggestionCursor: () => 0,
    working: () => false,
    usage: () => usage,
    focusRing: createFocusRing(() => ["stats"]),
  } as unknown as Parameters<typeof AppStateProvider>[0]["value"]
}

async function renderSetup(
  template: string,
  workspaceRoot = "/tmp/workspace",
  usage: UsageSnapshot | null = null,
  themeName = "mocha",
  align?: TextAlign,
) {
  const previous = process.env.MITOS_WORKSPACE_ROOT
  process.env.MITOS_WORKSPACE_ROOT = workspaceRoot
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: themeName })}>
        <AppStateProvider value={fakeAppState(usage)}>
          <TemplateWidget template={template} align={align} />
        </AppStateProvider>
      </ThemeProvider>
    ),
    { width: 50, height: 20 },
  )
  await setup.renderOnce()
  process.env.MITOS_WORKSPACE_ROOT = previous
  return setup
}

async function render(
  template: string,
  workspaceRoot = "/tmp/workspace",
  usage: UsageSnapshot | null = null,
) {
  const setup = await renderSetup(template, workspaceRoot, usage)
  const frame = setup.captureCharFrame()
  setup.renderer.destroy()
  return frame
}

function fgHex(fg: RGBA): string {
  const [r, g, b] = fg.toInts()
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`
}

/** Non-whitespace spans only, in order, with fg as `#rrggbb` — the padding
 * and border chrome around the panel are not this test's concern. */
function visibleSpans(setup: Awaited<ReturnType<typeof renderSetup>>) {
  return setup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0)
    .map((span) => ({ text: span.text, fg: fgHex(span.fg) }))
}

test("a hand-drawn template interpolates every referenced metric", async () => {
  const frame = await render("{{harness}}\n# {{thread.id}}  {{thread.status}}\n· created {{thread.created}}\n⌘ {{workspace}}")
  expect(frame).toContain("codex")
  expect(frame).toContain("a1b2c3d4")
  expect(frame).toContain("active")
  expect(frame).toContain("2026-10-01 08:48:08")
  expect(frame).toContain("/tmp/workspace")
})

const exampleUsage: UsageSnapshot = {
  harness: "opencode",
  observed_at: "2026-10-01T08:48:17Z",
  input_tokens: 123456,
  output_tokens: 7890,
  cached_input_tokens: 4321,
  cost_usd: 2.1016,
  context_used_tokens: 64000,
  context_limit_tokens: 128000,
  model: "glm-5.3",
  turns: 4,
  plan_five_hour_percent: null,
  plan_five_hour_resets_at: null,
  plan_week_percent: null,
  plan_week_resets_at: null,
}

const example = (name: string) =>
  readFileSync(join(import.meta.dir, `../../../examples/widgets/${name}`), "utf8")

const usageExample = example("usage.hbs")
const tokensExample = example("usage-tokens.hbs")
const limitsExample = example("usage-limits.hbs")

test("the tokens example shows thread-cumulative tokens and cost", async () => {
  const frame = await render(tokensExample, "/tmp/workspace", exampleUsage)
  expect(frame).toContain("this thread, all harnesses")
  expect(frame).toContain("in 123,456")
  expect(frame).toContain("out 7,890")
  expect(frame).toContain("cached 4,321")
  expect(frame).toContain("$2.10")
  expect(frame).toContain("50%")
})

test("the tokens example says so when no cost was reported", async () => {
  const frame = await render(tokensExample, "/tmp/workspace", {
    ...exampleUsage,
    cost_usd: null,
  })
  expect(frame).toMatch(/cost\s+not reported/)
})

test("the example usage panels wait for the first turn when there is no usage", async () => {
  for (const template of [usageExample, tokensExample]) {
    const frame = await render(template)
    expect(frame).toContain("waiting for the first turn")
  }
})

async function blockColumn(align: TextAlign | undefined): Promise<number> {
  const setup = await renderSetup("abcd\nab", "/tmp/workspace", null, "mocha", align)
  const frame = setup.captureCharFrame()
  setup.renderer.destroy()
  const row = frame.split("\n").find((line) => line.includes("abcd")) ?? ""
  return row.indexOf("abcd")
}

test("align places the whole text block left, centered or right", async () => {
  const left = await blockColumn(undefined)
  const center = await blockColumn(TextAlign.Center)
  const right = await blockColumn(TextAlign.Right)
  expect(await blockColumn(TextAlign.Left)).toBe(left)
  expect(center).toBeGreaterThan(left)
  expect(right).toBeGreaterThan(center)
  expect(right).toBe(50 - left - 4)
})

test("alignment keeps a block's lines together instead of aligning each line", async () => {
  const setup = await renderSetup("abcd\nab", "/tmp/workspace", null, "mocha", TextAlign.Right)
  const lines = setup.captureCharFrame().split("\n")
  setup.renderer.destroy()
  const top = lines.find((line) => line.includes("abcd")) ?? ""
  const next = lines[lines.indexOf(top) + 1] ?? ""
  expect(next.indexOf("ab")).toBe(top.indexOf("abcd"))
})

test("the logo example draws the wordmark in gold", async () => {
  const setup = await renderSetup(example("logo.hbs"))
  const spans = visibleSpans(setup)
  setup.renderer.destroy()
  const gold = spans.filter((s) => s.fg?.toLowerCase().startsWith("#d"))
  expect(gold.length).toBeGreaterThan(0)
  expect(spans.map((s) => s.text).join("")).toContain("the golden thread")
})

test("the usage example shows rounded limit percents with reset times", async () => {
  const frame = await render(usageExample, "/tmp/workspace", {
    ...exampleUsage,
    plan_five_hour_percent: 50.6,
    plan_five_hour_resets_at: new Date(Date.now() + 3_600_000).toISOString(),
    plan_week_percent: 63,
    plan_week_resets_at: new Date(Date.now() + 2 * 86_400_000).toISOString(),
  })
  expect(frame).toContain("Current session")
  expect(frame).toContain("51%")
  expect(frame).toContain("63%")
  expect(frame).toMatch(/resets \d{1,2}:\d{2}(am|pm)/)
  expect(frame).not.toContain("not reported")
})

test("the usage and limits examples fall back when limits are unreported", async () => {
  expect(await render(usageExample, "/tmp/workspace", exampleUsage)).toContain(
    "not reported",
  )
  const strip = await render(limitsExample, "/tmp/workspace", exampleUsage)
  expect(strip).toContain("5h")
  expect(strip).toContain("not reported")
})

test("static text in the template passes through untouched", async () => {
  const frame = await render("status: {{thread.status}} (custom label)")
  expect(frame).toContain("status: active (custom label)")
})

test("a reference to a metric with no current value interpolates as empty, not literally 'undefined'", async () => {
  const frame = await render("rate: [{{usage.plan_5h}}]")
  expect(frame).toContain("rate: []")
  expect(frame).not.toContain("undefined")
})

test("values are not HTML-escaped — this is terminal text, not markup", async () => {
  const frame = await render("{{workspace}}", "/tmp/a&b<c>")
  expect(frame).toContain("/tmp/a&b<c>")
  expect(frame).not.toContain("&amp;")
})

test("a {{#color}} block renders its text in the resolved theme color", async () => {
  const setup = await renderSetup('{{#color "accent"}}{{harness}}{{/color}} ({{thread.status}})')
  const spans = visibleSpans(setup)
  setup.renderer.destroy()
  expect(spans).toContainEqual({ text: "codex", fg: "#89b4fa" })
  expect(spans.find((s) => s.text.includes("active"))?.fg).toBe("#cdd6f4")
})

test("under the default theme a {{#color}} block is painted with the terminal's palette, not a fixed color", async () => {
  const setup = await renderSetup('{{#color "accent"}}{{harness}}{{/color}} plain', "/tmp/workspace", null, "default")
  const spans = setup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0)
  setup.renderer.destroy()
  const accent = spans.find((span) => span.text === "codex")
  expect(accent?.fg.intent).toBe("indexed")
  expect(accent?.fg.slot).toBe(4)
  const plain = spans.find((span) => span.text.includes("plain"))
  expect(plain?.fg.intent).toBe("default")
})

test("under the default theme the usage bar's severity color comes from the palette", async () => {
  const frameSetup = await renderSetup("{{bar usage.context_percent 10}}", "/tmp/workspace", { ...exampleUsage, context_used_tokens: 90, context_limit_tokens: 100 }, "default")
  const spans = frameSetup
    .captureSpans()
    .lines.flatMap((line) => line.spans)
    .filter((span) => span.text.trim().length > 0)
  frameSetup.renderer.destroy()
  const filled = spans.find((span) => span.text.includes("█"))
  const empty = spans.find((span) => span.text.includes("░"))
  expect(filled?.fg.slot).toBe(1)
  expect(empty?.fg.slot).toBe(8)
})

test("a {{#gradient}} block colors each character along the interpolation, not the default text color", async () => {
  const setup = await renderSetup('{{#gradient "accent" "err"}}{{thread.id}}{{/gradient}}')
  const spans = visibleSpans(setup)
  setup.renderer.destroy()
  const idSpans = spans.filter((s) => "a1b2c3d4".includes(s.text))
  expect(idSpans.map((s) => s.text).join("")).toBe("a1b2c3d4")
  expect(idSpans[0]?.fg).toBe("#89b4fa")
  expect(idSpans[idSpans.length - 1]?.fg).toBe("#f38ba8")
  const distinctColors = new Set(idSpans.map((s) => s.fg))
  expect(distinctColors.size).toBeGreaterThan(2)
})
