import { expect, test } from "bun:test";

import { installedWasmRoot } from "@theme/parsers.ts";

test("installedWasmRoot returns nothing when the parser package cannot resolve", () => {
  expect(
    installedWasmRoot(() => {
      throw new Error("missing parser package");
    }),
  ).toBeUndefined();
});
