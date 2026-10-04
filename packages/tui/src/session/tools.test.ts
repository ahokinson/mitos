import { expect, test } from "bun:test";

import { EventKind, type ThreadEvent } from "@session/events.ts";
import { ToolKind, foldedCallIds, toolCall, toolGlyph } from "@session/tools.ts";

function call(content: string | null, payload: unknown): ThreadEvent {
  return {
    thread_id: "t1",
    seq: 1,
    turn_id: null,
    harness: "claude",
    kind: EventKind.ToolCall,
    role: null,
    content,
    payload,
    created_at: "2026-01-01T00:00:00Z",
  };
}

const claude = (name: string, input: unknown) =>
  call(name, { type: "tool_use", name, input });

test("a Claude call takes its kind from the tool and its argument from the input", () => {
  expect(toolCall(claude("Bash", { command: "git status", description: "x" }))).toEqual({
    kind: ToolKind.Shell,
    name: "Bash",
    lines: ["git status"],
    hidden: 0,
  });
  expect(toolCall(claude("Read", { file_path: "src/a.ts", limit: 5 })).lines).toEqual(["src/a.ts"]);
  expect(toolCall(claude("Grep", { pattern: "foo", path: "src" })).lines).toEqual(["foo"]);
  expect(toolCall(claude("TodoWrite", { todos: [] }))).toMatchObject({ kind: ToolKind.Todo, lines: [] });
});

test("a long command keeps its first lines and counts the rest", () => {
  const command = Array.from({ length: 7 }, (_, i) => `line ${i + 1}`).join("\n");
  const shown = toolCall(claude("Bash", { command }));
  expect(shown.lines).toEqual(["line 1", "line 2", "line 3", "line 4"]);
  expect(shown.hidden).toBe(3);
});

test("other harnesses keep their content and infer the kind from the payload", () => {
  expect(toolCall(call("bun test", { type: "commandExecution" }))).toEqual({
    kind: ToolKind.Shell,
    name: null,
    lines: ["bun test"],
    hidden: 0,
  });
  expect(toolCall(call("1 file change", { type: "fileChange" })).kind).toBe(ToolKind.Edit);
  expect(toolCall(call("read: a.ts", { tool: "read" })).kind).toBe(ToolKind.Read);
  expect(toolCall(call("ls", { kind: "execute" })).kind).toBe(ToolKind.Shell);
  expect(toolCall(call("mystery", {})).kind).toBe(ToolKind.Other);
});

function withId(event: ThreadEvent, id: string): ThreadEvent {
  return { ...event, payload: { ...(event.payload as object), id } };
}

function patchResult(id: string, isError = false): ThreadEvent {
  return {
    ...call("ok", {
      type: "tool_result",
      tool_use_id: id,
      is_error: isError,
      tool_use_result: { filePath: "a.ts", structuredPatch: [] },
    }),
    kind: EventKind.ToolResult,
  };
}

test("a successful edit folds its call and a read of the same file just before it", () => {
  const read = withId(claude("Read", { file_path: "a.ts" }), "r1");
  const edit = withId(claude("Edit", { file_path: "a.ts" }), "e1");
  expect([...foldedCallIds([read, edit, patchResult("e1")])].sort()).toEqual(["e1", "r1"]);
});

test("a read stays when the edit failed, is unresolved, or targets another file", () => {
  const read = withId(claude("Read", { file_path: "a.ts" }), "r1");
  const edit = withId(claude("Edit", { file_path: "a.ts" }), "e1");
  const other = withId(claude("Edit", { file_path: "b.ts" }), "e2");
  expect(foldedCallIds([read, edit, patchResult("e1", true)]).size).toBe(0);
  expect(foldedCallIds([read, edit]).size).toBe(0);
  expect([...foldedCallIds([read, other, patchResult("e2")])]).toEqual(["e2"]);
});

test("a read separated from the edit by another call is kept", () => {
  const read = withId(claude("Read", { file_path: "a.ts" }), "r1");
  const bash = withId(claude("Bash", { command: "ls" }), "b1");
  const edit = withId(claude("Edit", { file_path: "a.ts" }), "e1");
  expect([...foldedCallIds([read, bash, edit, patchResult("e1")])]).toEqual(["e1"]);
});

test("every tool kind has a glyph", () => {
  for (const kind of Object.values(ToolKind)) expect(toolGlyph(kind)).toHaveLength(1);
});
