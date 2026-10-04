import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { ThreadMode } from "@session/threads.ts"
import { resolveTheme } from "@theme/palettes.ts"
import { ThemeProvider } from "@theme/providers.tsx"
import { Composer } from "@widgets/conversation/composers.tsx"

type Harness = {
  submitted: string[]
  drafts: string[]
  apply: ((text: string) => void) | null
}

async function mount(options: {
  working?: boolean
  mode?: ThreadMode
  hasSession?: boolean
  history?: readonly string[]
}) {
  const state: Harness = { submitted: [], drafts: [], apply: null }
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <Composer
          focused
          hasSession={options.hasSession ?? true}
          working={options.working ?? false}
          mode={options.mode ?? ThreadMode.Build}
          loadHistory={() => options.history ?? []}
          onSubmit={(text) => state.submitted.push(text)}
          onDraftChange={(text) => state.drafts.push(text)}
          registerSuggestionApplier={(apply) => {
            state.apply = apply
          }}
        />
      </ThemeProvider>
    ),
    { width: 40, height: 8 },
  )
  await setup.renderOnce()
  return { setup, state }
}

test("enter submits trimmed text and clears the draft", async () => {
  const { setup, state } = await mount({})
  try {
    await setup.mockInput.typeText("  hello there ")
    await setup.flush()
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(state.submitted).toEqual(["hello there"])
    expect(state.drafts.at(-1)).toBe("")
  } finally {
    setup.renderer.destroy()
  }
})

test("an empty draft submits nothing", async () => {
  const { setup, state } = await mount({})
  try {
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(state.submitted).toEqual([])
  } finally {
    setup.renderer.destroy()
  }
})

test("chat text is held while a turn runs, but commands go through", async () => {
  const { setup, state } = await mount({ working: true })
  try {
    await setup.mockInput.typeText("more please")
    await setup.flush()
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(state.submitted).toEqual([])

    await setup.mockInput.typeText("/approve")
    await setup.flush()
    state.apply?.("/approve")
    setup.mockInput.pressEnter()
    await setup.flush()
    expect(state.submitted).toEqual(["/approve"])
  } finally {
    setup.renderer.destroy()
  }
})

test("the suggestion applier replaces the draft and reports it", async () => {
  const { setup, state } = await mount({})
  try {
    state.apply?.("/archive ")
    await setup.flush()
    expect(state.drafts.at(-1)).toBe("/archive ")
  } finally {
    setup.renderer.destroy()
  }
})

test("up and down walk through earlier messages", async () => {
  const { setup, state } = await mount({ history: ["newest", "older"] })
  try {
    setup.mockInput.pressArrow("up")
    await setup.flush()
    expect(state.drafts.at(-1)).toBe("newest")
    setup.mockInput.pressArrow("up")
    await setup.flush()
    expect(state.drafts.at(-1)).toBe("older")
    setup.mockInput.pressArrow("down")
    await setup.flush()
    expect(state.drafts.at(-1)).toBe("newest")
  } finally {
    setup.renderer.destroy()
  }
})

test("history keys leave a non-empty draft alone", async () => {
  const { setup, state } = await mount({ history: ["old"] })
  try {
    await setup.mockInput.typeText("draft")
    await setup.flush()
    setup.mockInput.pressArrow("up")
    await setup.flush()
    expect(state.drafts.at(-1)).toBe("draft")
  } finally {
    setup.renderer.destroy()
  }
})

test("plan mode and a fresh thread are labelled", async () => {
  const { setup } = await mount({ mode: ThreadMode.Plan, hasSession: false })
  try {
    const frame = setup.captureCharFrame()
    expect(frame).toContain("plan ›")
    expect(frame).toContain("Start a conversation…")
  } finally {
    setup.renderer.destroy()
  }
})
