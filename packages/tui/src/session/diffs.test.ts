import { expect, test } from "bun:test";
import { diffTexts, payloadDiffs } from "@session/diffs.ts";

test("diffTexts is null for identical texts", () => {
  expect(diffTexts("a.ts", "same\n", "same\n")).toBeNull();
});

test("diffTexts reports a replaced line with context and counts", () => {
  const diff = diffTexts("a.ts", "one\ntwo\nthree\n", "one\n2\nthree\n");
  expect(diff).not.toBeNull();
  expect(diff?.added).toBe(1);
  expect(diff?.removed).toBe(1);
  expect(diff?.diff).toBe(
    [
      "--- a/a.ts",
      "+++ b/a.ts",
      "@@ -1,3 +1,3 @@",
      " one",
      "-two",
      "+2",
      " three",
      "",
    ].join("\n"),
  );
});

test("diffTexts keeps far-apart changes in separate hunks", () => {
  const before = Array.from({ length: 20 }, (_, i) => `l${i}`).join("\n");
  const after = before.replace("l1\n", "L1\n").replace("l18\n", "L18\n");
  const diff = diffTexts("a.ts", before, after);
  expect(diff?.diff.match(/^@@/gm)).toHaveLength(2);
  expect(diff?.added).toBe(2);
  expect(diff?.removed).toBe(2);
});

test("diffTexts treats a new file as all additions", () => {
  const diff = diffTexts("new.ts", "", "a\nb\n");
  expect(diff?.added).toBe(2);
  expect(diff?.removed).toBe(0);
  expect(diff?.diff).toContain("@@ -1,0 +1,2 @@");
});

test("diffTexts caps long output and recomputes hunk counts", () => {
  const content = Array.from({ length: 200 }, (_, i) => `line ${i}`).join("\n");
  const diff = diffTexts("big.ts", "", content);
  expect(diff?.added).toBe(200);
  expect(diff?.truncated).toBe(140);
  expect(diff?.diff).toContain("@@ -1,0 +1,60 @@");
  expect(
    diff?.diff.split("\n").filter((l) => l.startsWith("+line")),
  ).toHaveLength(60);
});

test("claude Edit, MultiEdit and Write tool_use blocks yield diffs", () => {
  const edit = payloadDiffs({
    type: "tool_use",
    name: "Edit",
    input: { file_path: "/p/a.ts", old_string: "x", new_string: "y" },
  });
  expect(edit).toHaveLength(1);
  expect(edit[0]?.path).toBe("/p/a.ts");
  expect(edit[0]?.diff).toContain("-x\n+y");

  const multi = payloadDiffs({
    type: "tool_use",
    name: "MultiEdit",
    input: {
      file_path: "/p/a.ts",
      edits: [
        { old_string: "a", new_string: "b" },
        { old_string: "c", new_string: "d" },
      ],
    },
  });
  expect(multi).toHaveLength(1);
  expect(multi[0]?.added).toBe(2);
  expect(multi[0]?.removed).toBe(2);

  const write = payloadDiffs({
    type: "tool_use",
    name: "Write",
    input: { file_path: "/p/w.ts", content: "hello\n" },
  });
  expect(write[0]?.added).toBe(1);
});

test("claude non-edit tools yield nothing", () => {
  expect(
    payloadDiffs({ type: "tool_use", name: "Bash", input: { command: "ls" } }),
  ).toEqual([]);
  expect(
    payloadDiffs({
      type: "tool_use",
      name: "Read",
      input: { file_path: "/p/a" },
    }),
  ).toEqual([]);
});

test("codex fileChange uses unified diffs and raw content for add/delete", () => {
  const diffs = payloadDiffs({
    type: "fileChange",
    changes: [
      {
        path: "a.ts",
        kind: { type: "update" },
        diff: "@@ -1,2 +1,2 @@\n keep\n-old\n+new\n",
      },
      { path: "b.ts", kind: { type: "add" }, diff: "fresh\n" },
      { path: "c.ts", kind: { type: "delete" }, diff: "gone\n" },
    ],
  });
  expect(diffs.map((d) => [d.path, d.added, d.removed])).toEqual([
    ["a.ts", 1, 1],
    ["b.ts", 1, 0],
    ["c.ts", 0, 1],
  ]);
});

test("opencode prefers metadata.diff and falls back to input", () => {
  const fromMetadata = payloadDiffs({
    type: "tool",
    tool: "edit",
    state: {
      input: { filePath: "a.ts", oldString: "x", newString: "y" },
      metadata: { diff: "@@ -1 +1 @@\n-x\n+y\n" },
    },
  });
  expect(fromMetadata[0]?.diff).toContain("@@ -1,1 +1,1 @@");

  const fromInput = payloadDiffs({
    type: "tool",
    tool: "edit",
    state: { input: { filePath: "a.ts", oldString: "x", newString: "y" } },
  });
  expect(fromInput[0]?.added).toBe(1);

  const write = payloadDiffs({
    type: "tool",
    tool: "write",
    state: { input: { filePath: "n.ts", content: "a\nb" } },
  });
  expect(write[0]?.added).toBe(2);

  expect(
    payloadDiffs({
      type: "tool",
      tool: "bash",
      state: { input: { command: "ls" } },
    }),
  ).toEqual([]);
});

test("hermes diff content blocks yield diffs, null oldText means new file", () => {
  const diffs = payloadDiffs({
    toolCallId: "t1",
    content: [
      { type: "content", content: { type: "text", text: "hi" } },
      { type: "diff", path: "a.ts", oldText: "x\n", newText: "y\n" },
      { type: "diff", path: "n.ts", oldText: null, newText: "z\n" },
    ],
  });
  expect(diffs.map((d) => [d.path, d.added, d.removed])).toEqual([
    ["a.ts", 1, 1],
    ["n.ts", 1, 0],
  ]);
});

test("unrecognised payloads yield nothing", () => {
  expect(payloadDiffs(null)).toEqual([]);
  expect(payloadDiffs("text")).toEqual([]);
  expect(payloadDiffs([])).toEqual([]);
  expect(payloadDiffs({})).toEqual([]);
});
