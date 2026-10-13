import { expect, test } from "bun:test";
import { LayoutDirection } from "@layout/configs.ts";
import { type TileNode, tileChildStyle } from "@layout/tiles.tsx";

test("a child with no size flexGrows to fill remaining space", () => {
  expect(tileChildStyle(LayoutDirection.Row, undefined)).toEqual({
    flexGrow: 1,
  });
});

test("a sized child in a row gets an explicit width", () => {
  expect(tileChildStyle(LayoutDirection.Row, "30%")).toEqual({ width: "30%" });
});

test("a sized child in a column gets an explicit height", () => {
  expect(tileChildStyle(LayoutDirection.Column, 10)).toEqual({ height: 10 });
});

test("TileNode<T> accepts a generic leaf payload, not just a panel id string", () => {
  const tile: TileNode<{ label: string }> = {
    direction: LayoutDirection.Row,
    gap: 2,
    children: [
      { leaf: { label: "left" }, size: "50%" },
      { leaf: { label: "right" } },
    ],
  };
  expect("leaf" in tile).toBe(false);
  if (!("leaf" in tile)) {
    expect(tile.children).toHaveLength(2);
    expect(tile.children[0]).toEqual({ leaf: { label: "left" }, size: "50%" });
  }
});
