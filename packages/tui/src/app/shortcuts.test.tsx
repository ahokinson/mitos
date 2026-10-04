import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { useAppShortcuts } from "@app/shortcuts.ts"
import type { SuggestionsState } from "@commands/suggestionStates.ts"
import type { CommandSuggestion } from "@commands/suggestions.ts"
import { createFocusRing } from "@input/focusRings.ts"
import { resolveKeyBindings } from "@input/keybindings.ts"
import { DEFAULT_LAYOUT_CONFIG, type LayoutConfig, WidgetKind } from "@layout/trees.ts"

type Calls = {
  moved: number[]
  accepted: number
  dismissed: number
  newThread: number
  refreshed: number
  quit: number
}

function fakeSuggestions(options: { items: readonly CommandSuggestion[]; completes?: boolean; acceptResult?: boolean }, calls: Calls): SuggestionsState {
  return {
    setDraft: () => {},
    suggestions: () => options.items,
    cursor: () => 0,
    move: (delta) => calls.moved.push(delta),
    accept: () => {
      calls.accepted += 1
      return options.acceptResult ?? true
    },
    dismiss: () => {
      calls.dismissed += 1
    },
    completesDraft: () => options.completes ?? false,
    registerApplier: () => {},
  }
}

const ITEM: CommandSuggestion = { label: "/help", insertText: "/help ", hint: "help" }

const TWO_PANES: LayoutConfig = {
  widgets: {
    one: { kind: WidgetKind.Conversation, focusKey: "ctrl+a" },
    two: { kind: WidgetKind.Conversation, focusKey: "ctrl+b" },
  },
  tree: { direction: "row" as never, children: [{ widget: "one" }, { widget: "two" }] },
}

async function mount(options: { items?: readonly CommandSuggestion[]; completes?: boolean; acceptResult?: boolean; layout?: LayoutConfig }) {
  const calls: Calls = { moved: [], accepted: 0, dismissed: 0, newThread: 0, refreshed: 0, quit: 0 }
  let ring!: ReturnType<typeof createFocusRing>
  const setup = await testRender(
    () => {
      const layout = options.layout ?? DEFAULT_LAYOUT_CONFIG
      ring = createFocusRing(() => Object.keys(layout.widgets))
      useAppShortcuts({
        suggestions: fakeSuggestions({ items: options.items ?? [], completes: options.completes, acceptResult: options.acceptResult }, calls),
        keyBindings: () => resolveKeyBindings(undefined),
        layout: () => layout,
        focusRing: ring,
        startNewThread: () => {
          calls.newThread += 1
        },
        refresh: () => {
          calls.refreshed += 1
        },
        quit: () => {
          calls.quit += 1
        },
      })
      return <box />
    },
    { width: 10, height: 4 },
  )
  await setup.renderOnce()
  return { setup, calls, ring: () => ring }
}

test("configured actions fire their callbacks", async () => {
  const { setup, calls } = await mount({})
  try {
    setup.mockInput.pressKey("n", { ctrl: true })
    setup.mockInput.pressKey("l", { ctrl: true })
    setup.mockInput.pressKey("c", { ctrl: true })
    await setup.flush()
    expect(calls).toMatchObject({ newThread: 1, refreshed: 1, quit: 1 })
  } finally {
    setup.renderer.destroy()
  }
})

test("tab and shift+tab cycle focus", async () => {
  const { setup, ring } = await mount({ layout: TWO_PANES })
  try {
    expect(ring().current()).toBe("one")
    setup.mockInput.pressTab()
    await setup.flush()
    expect(ring().current()).toBe("two")
    setup.mockInput.pressTab({ shift: true })
    await setup.flush()
    expect(ring().current()).toBe("one")
  } finally {
    setup.renderer.destroy()
  }
})

test("a pane's focus key jumps straight to it", async () => {
  const { setup, ring } = await mount({ layout: TWO_PANES })
  try {
    setup.mockInput.pressKey("b", { ctrl: true })
    await setup.flush()
    expect(ring().current()).toBe("two")
  } finally {
    setup.renderer.destroy()
  }
})

test("arrows move through suggestions instead of the draft", async () => {
  const { setup, calls } = await mount({ items: [ITEM] })
  try {
    setup.mockInput.pressArrow("down")
    setup.mockInput.pressArrow("up")
    await setup.flush()
    expect(calls.moved).toEqual([1, -1])
  } finally {
    setup.renderer.destroy()
  }
})

test("tab and enter accept the highlighted suggestion", async () => {
  const { setup, calls } = await mount({ items: [ITEM] })
  try {
    setup.mockInput.pressTab()
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(calls.accepted).toBe(2)
  } finally {
    setup.renderer.destroy()
  }
})

test("enter on a fully typed command is left to submit it", async () => {
  const { setup, calls } = await mount({ items: [ITEM], completes: true })
  try {
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(calls.accepted).toBe(0)
  } finally {
    setup.renderer.destroy()
  }
})

test("a suggestion that cannot be applied falls through to normal handling", async () => {
  const { setup, calls } = await mount({ items: [ITEM], acceptResult: false })
  try {
    setup.mockInput.pressTab()
    await setup.flush()
    expect(calls.accepted).toBe(1)
  } finally {
    setup.renderer.destroy()
  }
})

test("escape dismisses suggestions", async () => {
  const { setup, calls } = await mount({ items: [ITEM] })
  try {
    setup.mockInput.pressEscape()
    await new Promise((resolve) => setTimeout(resolve, 50))
    await setup.flush()
    expect(calls.dismissed).toBe(1)
  } finally {
    setup.renderer.destroy()
  }
})
