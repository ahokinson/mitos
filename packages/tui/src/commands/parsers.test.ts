import { expect, test } from "bun:test";

import { ParsedKind, parseCommandInput } from "@commands/parsers.ts";

test("plain text is chat", () => {
  expect(parseCommandInput("hello there")).toEqual({
    kind: ParsedKind.Chat,
    text: "hello there",
  });
});

test("a leading slash is a command", () => {
  expect(parseCommandInput("/new codex")).toEqual({
    kind: ParsedKind.Command,
    command: { name: "new", args: ["codex"], argLine: "codex" },
  });
});

test("command name is lowercased, args/argLine split on whitespace", () => {
  expect(parseCommandInput("/Resume some query here")).toEqual({
    kind: ParsedKind.Command,
    command: {
      name: "resume",
      args: ["some", "query", "here"],
      argLine: "some query here",
    },
  });
});

test("a bare command with no args has empty args and argLine", () => {
  expect(parseCommandInput("/help")).toEqual({
    kind: ParsedKind.Command,
    command: { name: "help", args: [], argLine: "" },
  });
});

test("a double leading slash escapes to a literal chat message", () => {
  expect(parseCommandInput("//not-a-command")).toEqual({
    kind: ParsedKind.Chat,
    text: "/not-a-command",
  });
});

test("extra whitespace between the command name and args is collapsed", () => {
  expect(parseCommandInput("/delete   confirm   abc123")).toEqual({
    kind: ParsedKind.Command,
    command: {
      name: "delete",
      args: ["confirm", "abc123"],
      argLine: "confirm   abc123",
    },
  });
});
