import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { resolveTheme } from "@theme/palettes.ts"
import { ThemeProvider } from "@theme/providers.tsx"
import { WorkingIndicator } from "@widgets/conversation/workingIndicators.tsx"

async function frameOf(awaitingAnswer: boolean): Promise<string> {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <WorkingIndicator awaitingAnswer={awaitingAnswer} />
      </ThemeProvider>
    ),
    { width: 40, height: 4 },
  )
  try {
    await setup.renderOnce()
    return setup.captureCharFrame()
  } finally {
    setup.renderer.destroy()
  }
}

test("a running turn shows it is working", async () => {
  expect(await frameOf(false)).toContain("working")
})

test("a turn blocked on a request says it is waiting for an answer", async () => {
  expect(await frameOf(true)).toContain("waiting for your answer")
})
