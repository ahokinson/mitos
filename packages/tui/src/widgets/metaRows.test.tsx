import { expect, test } from "bun:test"
import { testRender } from "@opentui/solid"

import { renderTile, type TileNode } from "@layout/tiles.tsx"
import { LayoutDirection } from "@layout/trees.ts"
import { MetaRow } from "@widgets/metaRows.tsx"
import { ThemeProvider } from "@theme/providers.tsx"
import { resolveTheme } from "@theme/palettes.ts"

type Leaf = () => unknown
function leafBox(content: Leaf, style: Record<string, unknown>) {
  return (
    <box {...style} flexDirection="column">
      {content()}
    </box>
  )
}

test("an inline MetaRow puts label and value on one line, in a custom color", async () => {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <box width={30} flexDirection="column">
          <MetaRow label="status" value="active" valueColor="#ff0000" />
          <MetaRow label="mode" value="plan" />
        </box>
      </ThemeProvider>
    ),
    { width: 32, height: 4 },
  )

  try {
    await setup.renderOnce()
    const lines = setup.captureCharFrame().split("\n")
    expect(lines[0]).toMatch(/status\s+active/)
    expect(lines[1]).toMatch(/mode\s+plan/)
  } finally {
    setup.renderer.destroy()
  }
})

test("stacked MetaRow values stay intact across two tiled columns, never bleeding into the sibling", async () => {
  const tile: TileNode<Leaf> = {
    direction: LayoutDirection.Row,
    gap: 2,
    children: [
      {
        size: "35%",
        leaf: () => (
          <>
            <MetaRow stacked label="thread" value="a1b2c3d4" />
            <MetaRow stacked label="status" value="active" />
          </>
        ),
      },
      {
        leaf: () => (
          <>
            <MetaRow stacked label="created" value="2026-10-01 10:00:00" />
            <MetaRow stacked label="updated" value="2026-10-02 09:30:00" />
          </>
        ),
      },
    ],
  }

  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <box width={40} flexDirection="column">
          {renderTile(tile, leafBox, { width: "100%" })}
        </box>
      </ThemeProvider>
    ),
    { width: 42, height: 10 },
  )

  try {
    await setup.renderOnce()
    const frame = setup.captureCharFrame()
    // The actual bug: flexGrow/flexBasis on a text node inside a
    // column-direction box (the wrong axis for those props) corrupted the
    // layout, dropping the preceding column's last character and running
    // it straight into the next column's text with no gap.
    expect(frame).toContain("a1b2c3d4")
    expect(frame).toContain("2026-10-01 10:00:00")
    expect(frame).toContain("2026-10-02 09:30:00")
    expect(frame).not.toContain("a1b2c3d2026")
  } finally {
    setup.renderer.destroy()
  }
})
