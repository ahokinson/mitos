import { expect, test } from "bun:test";

import { createInputHistory } from "@widgets/conversation/histories.ts";

const ENTRIES = ["third", "second", "first"];

test("steps back from an empty composer and forward to empty", () => {
  const history = createInputHistory(() => ENTRIES);
  expect(history.prev("")).toBe("third");
  expect(history.prev("third")).toBe("second");
  expect(history.prev("second")).toBe("first");
  expect(history.prev("first")).toBeUndefined();
  expect(history.next("first")).toBe("second");
  expect(history.next("second")).toBe("third");
  expect(history.next("third")).toBe("");
  expect(history.next("")).toBeUndefined();
});

test("ignores Up in a typed draft", () => {
  const history = createInputHistory(() => ENTRIES);
  expect(history.prev("draft")).toBeUndefined();
});

test("editing a recalled entry ends browsing", () => {
  const history = createInputHistory(() => ENTRIES);
  expect(history.prev("")).toBe("third");
  expect(history.prev("third!")).toBeUndefined();
  expect(history.next("third!")).toBeUndefined();
});

test("empty history recalls nothing", () => {
  const history = createInputHistory(() => []);
  expect(history.prev("")).toBeUndefined();
});

test("reloads entries on each fresh start", () => {
  let entries = ["one"];
  const history = createInputHistory(() => entries);
  expect(history.prev("")).toBe("one");
  expect(history.next("one")).toBe("");
  entries = ["two", "one"];
  expect(history.prev("")).toBe("two");
});

test("reset abandons browsing", () => {
  const history = createInputHistory(() => ENTRIES);
  history.prev("");
  history.reset();
  expect(history.next("third")).toBeUndefined();
});
