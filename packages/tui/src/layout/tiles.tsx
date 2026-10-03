import type { JSX } from "solid-js"

import { LayoutDirection, type PaneSize, type PaneStyle } from "@layout/trees.ts"

/** The same recursive row/column/children shape `[layout]` in `mitos.toml`
 * uses to arrange top-level widgets, generalized over the leaf payload —
 * `T` is a named widget id at the app level (`layout/index.tsx`), but can
 * just as well be a widget's own internal content blocks. This is what
 * makes "complex tiling" (nested row/column splits, not just a flat list)
 * available inside a widget's own body, not only at the app layout level. */
export type TileNode<T> =
  | { leaf: T; size?: PaneSize }
  | { direction: LayoutDirection; size?: PaneSize; gap?: number; children: readonly TileNode<T>[] }

export function tileChildStyle(parentDirection: LayoutDirection, size: PaneSize | undefined): PaneStyle {
  if (size === undefined) return { flexGrow: 1 }
  return parentDirection === LayoutDirection.Row ? { width: size } : { height: size }
}

/** Walks a `TileNode<T>` tree, handing each leaf's payload and computed
 * `PaneStyle` to `renderLeaf` — identical recursion to the app-level layout
 * renderer, just parameterized over what a leaf actually is. */
export function renderTile<T>(node: TileNode<T>, renderLeaf: (leaf: T, style: PaneStyle) => JSX.Element, style: PaneStyle): JSX.Element {
  if ("leaf" in node) return renderLeaf(node.leaf, style)
  return (
    <box flexDirection={node.direction} gap={node.gap} {...style}>
      {node.children.map((child) => renderTile(child, renderLeaf, tileChildStyle(node.direction, child.size)))}
    </box>
  )
}
