import { createSignal } from "solid-js"
import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { CommandSuggestions } from "@widgets/conversation/suggestionLists.tsx"
import { Composer } from "@widgets/conversation/composers.tsx"
import { WrappingText } from "@widgets/conversation/wrappingTexts.tsx"
import { ThreadMode } from "@session/threads.ts"
import { ThemeProvider } from "@theme/providers.tsx"
import { resolveTheme } from "@theme/palettes.ts"

test("transcript content wraps inside the space left by its glyph", async () => {
  const setup = await testRender(
    () => (
      <box width={14} flexDirection="row">
        <text width={1} flexShrink={0}>
          ›
        </text>
        <WrappingText color={undefined} content="messages wrap at narrow terminal widths" />
      </box>
    ),
    { width: 14, height: 6 },
  )

  try {
    await setup.renderOnce()
    const lines = setup.captureCharFrame().split("\n").filter((line) => line.trim().length > 0)
    expect(lines).toHaveLength(4)
    expect(lines[0]).toBe("›messages wrap")
    expect(lines[1]).toBe(" at narrow    ")
    expect(lines[2]).toBe(" terminal     ")
    expect(lines[3]).toBe(" widths       ")
  } finally {
    setup.renderer.destroy()
  }
})

test("composer starts at one row and expands only after soft wrapping", async () => {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <Composer focused hasSession working={false} mode={ThreadMode.Build} loadHistory={() => []} onSubmit={() => {}} onDraftChange={() => {}} registerSuggestionApplier={() => {}} />
      </ThemeProvider>
    ),
    { width: 22, height: 8 },
  )

  try {
    await setup.renderOnce()
    expect(setup.captureCharFrame().split("\n").filter((line) => line.trim().length > 0)).toHaveLength(3)

    await setup.mockInput.typeText("this composer expands only when text wraps")
    await setup.flush()
    expect(setup.captureCharFrame().split("\n").filter((line) => line.trim().length > 0).length).toBeGreaterThan(3)
  } finally {
    setup.renderer.destroy()
  }
})

test("command suggestions never bleed into each other when squeezed between flex siblings", async () => {
  const items = [
    { label: "/archive", insertText: "/archive ", hint: "Archive a thread" },
    { label: "/decision", insertText: "/decision ", hint: "Record a decision" },
    { label: "/delete", insertText: "/delete ", hint: "Permanently delete a thread (irreversible) — requires confirmation" },
    { label: "/harness", insertText: "/harness ", hint: "Reassign the selected thread's harness" },
    { label: "/help", insertText: "/help ", hint: "List available commands" },
  ]
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <box flexDirection="column" width={40} height={10}>
          <box flexGrow={1} flexShrink={1} minHeight={0}>
            <text>scrollback area</text>
          </box>
          <CommandSuggestions items={items} cursor={0} />
          <box flexShrink={0} height={3}>
            <text>composer area</text>
          </box>
        </box>
      </ThemeProvider>
    ),
    { width: 40, height: 10 },
  )

  try {
    await setup.renderOnce()
    const frame = setup.captureCharFrame()
    // Every label must appear intact and followed by a space, not run
    // straight into its own hint text (the original bug: flex-shrink spread
    // across both the label and hint text nodes instead of only the hint,
    // silently dropping the label's last character).
    for (const item of items) expect(frame).toContain(`${item.label} `)
    // The composer area must still get its own reserved space below the
    // suggestions box, never overlapped by it.
    expect(frame).toContain("composer area")
  } finally {
    setup.renderer.destroy()
  }
})

test("command suggestions scroll to keep the active row visible instead of capping the list", async () => {
  const items = Array.from({ length: 10 }, (_, i) => ({ label: `/cmd${i}`, insertText: `/cmd${i} `, hint: `hint ${i}` }))
  const [cursor, setCursor] = createSignal(0)
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <CommandSuggestions items={items} cursor={cursor()} />
      </ThemeProvider>
    ),
    { width: 40, height: 20 },
  )

  try {
    await setup.renderOnce()
    // Only a window of items is visible at once, but every one of the 10
    // exists in the data (suggestFor is uncapped) — scrolling, not
    // truncation, is how the rest become reachable.
    expect(setup.captureCharFrame()).not.toContain("/cmd9")

    setCursor(9)
    await setup.flush()
    expect(setup.captureCharFrame()).toContain("/cmd9")

    setCursor(0)
    await setup.flush()
    expect(setup.captureCharFrame()).toContain("/cmd0")
  } finally {
    setup.renderer.destroy()
  }
})
