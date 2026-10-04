import { expect, test } from "bun:test";

import {
  foldedCallIds,
  isQuietResult,
  type ToolFacts,
  ToolKind,
  ToolShape,
  toolCall,
  toolGlyph,
  toolTone,
  toolUseId,
} from "@session/tools.ts";
import { Tone } from "@theme/themes.ts";

function facts(overrides: Partial<ToolFacts>): ToolFacts {
  return {
    kind: ToolKind.Other,
    name: null,
    argument: "",
    tool_use_id: null,
    path: null,
    shape: ToolShape.Other,
    quiet: false,
    patched: false,
    ...overrides,
  };
}

const withFacts = (
  overrides: Partial<ToolFacts>,
  content: string | null = null,
) => ({
  content,
  tool: facts(overrides),
});

test("a Claude call shows its tool name and the head of its argument", () => {
  expect(
    toolCall(
      withFacts(
        {
          kind: ToolKind.Shell,
          name: "Bash",
          argument: "git status",
          shape: ToolShape.ToolUse,
        },
        "Bash",
      ),
    ),
  ).toEqual({
    kind: ToolKind.Shell,
    name: "Bash",
    lines: ["git status"],
    hidden: 0,
  });
  expect(
    toolCall(
      withFacts({
        kind: ToolKind.Todo,
        name: "TodoWrite",
        shape: ToolShape.ToolUse,
      }),
    ),
  ).toMatchObject({ kind: ToolKind.Todo, lines: [] });
});

test("a long command keeps its first lines and counts the rest", () => {
  const argument = Array.from({ length: 7 }, (_, i) => `line ${i + 1}`).join(
    "\n",
  );
  const shown = toolCall(
    withFacts({
      kind: ToolKind.Shell,
      name: "Bash",
      argument,
      shape: ToolShape.ToolUse,
    }),
  );
  expect(shown.lines).toEqual(["line 1", "line 2", "line 3", "line 4"]);
  expect(shown.hidden).toBe(3);
});

test("an over-long line is cut with an ellipsis", () => {
  const shown = toolCall(
    withFacts({ kind: ToolKind.Shell, argument: "x".repeat(500) }),
  );
  expect(shown.lines[0]).toHaveLength(401);
  expect(shown.lines[0]?.endsWith("…")).toBe(true);
});

test("a Claude file path is shortened against the working directory", () => {
  const path = `${process.cwd()}/src/a.ts`;
  const shown = toolCall(
    withFacts({
      kind: ToolKind.Read,
      name: "Read",
      argument: path,
      shape: ToolShape.ToolUse,
    }),
  );
  expect(shown.lines).toEqual(["src/a.ts"]);
  const other = toolCall(withFacts({ kind: ToolKind.Read, argument: path }));
  expect(other.lines).toEqual([path]);
});

test("other harnesses keep their content as the argument and have no name", () => {
  expect(
    toolCall(
      withFacts({ kind: ToolKind.Shell, argument: "bun test" }, "bun test"),
    ),
  ).toEqual({
    kind: ToolKind.Shell,
    name: null,
    lines: ["bun test"],
    hidden: 0,
  });
});

test("an event without facts falls back to its content", () => {
  expect(toolCall({ content: "mystery" })).toEqual({
    kind: ToolKind.Other,
    name: null,
    lines: ["mystery"],
    hidden: 0,
  });
});

function use(id: string, kind: ToolKind, path: string | null = null) {
  return {
    tool: facts({ kind, tool_use_id: id, path, shape: ToolShape.ToolUse }),
  };
}

function patchResult(id: string, patched = true) {
  return {
    tool: facts({ shape: ToolShape.ToolResult, tool_use_id: id, patched }),
  };
}

test("a successful edit folds its call and a read of the same file just before it", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const edit = use("e1", ToolKind.Edit, "a.ts");
  expect([...foldedCallIds([read, edit, patchResult("e1")])].sort()).toEqual([
    "e1",
    "r1",
  ]);
});

test("a read stays when the edit failed, is unresolved, or targets another file", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const edit = use("e1", ToolKind.Edit, "a.ts");
  const other = use("e2", ToolKind.Edit, "b.ts");
  expect(foldedCallIds([read, edit, patchResult("e1", false)]).size).toBe(0);
  expect(foldedCallIds([read, edit]).size).toBe(0);
  expect([...foldedCallIds([read, other, patchResult("e2")])]).toEqual(["e2"]);
});

test("a read separated from the edit by another call is kept", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const bash = use("b1", ToolKind.Shell);
  const edit = use("e1", ToolKind.Edit, "a.ts");
  expect([...foldedCallIds([read, bash, edit, patchResult("e1")])]).toEqual([
    "e1",
  ]);
});

test("a non-tool event between a read and an edit keeps the read", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const edit = use("e1", ToolKind.Edit, "a.ts");
  expect([...foldedCallIds([read, {}, edit, patchResult("e1")])]).toEqual([
    "e1",
  ]);
});

test("a call without an id never folds and breaks the read-edit pairing", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const anonymous = {
    tool: facts({
      kind: ToolKind.Edit,
      path: "a.ts",
      shape: ToolShape.ToolUse,
    }),
  };
  const edit = use("e1", ToolKind.Edit, "a.ts");
  expect([
    ...foldedCallIds([read, anonymous, edit, patchResult("e1")]),
  ]).toEqual(["e1"]);
});

test("a write after a read of the same file folds the read", () => {
  const read = use("r1", ToolKind.Read, "a.ts");
  const write = use("w1", ToolKind.Write, "a.ts");
  expect([...foldedCallIds([read, write, patchResult("w1")])].sort()).toEqual([
    "r1",
    "w1",
  ]);
});

test("every tool kind has a glyph", () => {
  for (const kind of Object.values(ToolKind))
    expect(toolGlyph(kind)).toHaveLength(1);
});

test("every tool kind has a tone", () => {
  expect(toolTone(ToolKind.Shell)).toBe(Tone.Warning);
  expect(toolTone(ToolKind.Edit)).toBe(Tone.Success);
  expect(toolTone(ToolKind.Write)).toBe(Tone.Success);
  expect(toolTone(ToolKind.Read)).toBe(Tone.Accent);
  expect(toolTone(ToolKind.Search)).toBe(Tone.Accent);
  expect(toolTone(ToolKind.Fetch)).toBe(Tone.Accent);
  expect(toolTone(ToolKind.Agent)).toBe(Tone.Muted);
  expect(toolTone(ToolKind.Todo)).toBe(Tone.Muted);
  expect(toolTone(ToolKind.Other)).toBe(Tone.Muted);
});

test("the tool use id and quiet flag come from the facts", () => {
  expect(toolUseId(use("b1", ToolKind.Shell))).toBe("b1");
  expect(toolUseId({})).toBeNull();
  expect(isQuietResult({ tool: facts({ quiet: true }) })).toBe(true);
  expect(isQuietResult({ tool: facts({}) })).toBe(false);
  expect(isQuietResult({})).toBe(false);
});
