import { expect, test } from "bun:test";

import { sessionColumns } from "@panels/charts/columns.ts";

test("session columns keep tokens when the pane fits every column", () => {
  expect(sessionColumns(40)).toEqual({
    title: 8,
    harness: true,
    tokens: true,
  });
});

test("session columns drop the harness and tokens in a tight pane", () => {
  expect(sessionColumns(24)).toEqual({
    title: 9,
    harness: false,
    tokens: false,
  });
});
