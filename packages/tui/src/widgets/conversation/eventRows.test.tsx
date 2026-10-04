import { expect, test } from "bun:test";
import { testRender } from "@opentui/solid";

import { EventKind, type ThreadEvent } from "@session/events.ts";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";
import { EventRow } from "@widgets/conversation/eventRows.tsx";

function toolEvent(kind: EventKind, payload: unknown): ThreadEvent {
  return {
    thread_id: "t1",
    seq: 1,
    turn_id: "turn1",
    harness: "claude",
    kind,
    role: null,
    content: "Edit",
    payload,
    created_at: "2026-01-01T00:00:00Z",
  };
}

const editPayload = {
  type: "tool_use",
  name: "Edit",
  input: { file_path: "src/a.ts", old_string: "oldValue", new_string: "newValue" },
};

async function frameOf(event: ThreadEvent): Promise<string> {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <EventRow event={event} />
      </ThemeProvider>
    ),
    { width: 60, height: 14 },
  );
  try {
    await setup.renderOnce();
    return setup.captureCharFrame();
  } finally {
    setup.renderer.destroy();
  }
}

const resultPayload = {
  type: "tool_result",
  tool_use_result: {
    filePath: "src/a.ts",
    structuredPatch: [{ oldStart: 1, newStart: 1, lines: ["-oldValue", "+newValue"] }],
  },
};

test("a file-edit tool result renders its header and diff, not the confirmation", async () => {
  const frame = await frameOf({
    ...toolEvent(EventKind.ToolResult, resultPayload),
    content: "The file src/a.ts has been updated successfully.",
  });
  expect(frame).toContain("± src/a.ts  +1 -1");
  expect(frame).toContain("oldValue");
  expect(frame).toContain("newValue");
  expect(frame).not.toContain("updated successfully");
});

test("a read result is hidden unless it failed", async () => {
  const read = { type: "tool_result", tool_use_result: { type: "text", file: { filePath: "a.ts" } } };
  const ok = await frameOf({ ...toolEvent(EventKind.ToolResult, read), content: "1 mod processes;" });
  expect(ok).not.toContain("mod processes");
  const failed = await frameOf({
    ...toolEvent(EventKind.ToolResult, { ...read, is_error: true }),
    content: "File does not exist.",
  });
  expect(failed).toContain("File does not exist.");
});

test("a shell result shows its head and how many lines were left out", async () => {
  const content = Array.from({ length: 7 }, (_, i) => `out ${i + 1}`).join("\n");
  const frame = await frameOf({ ...toolEvent(EventKind.ToolResult, { type: "tool_result" }), content });
  expect(frame).toContain("out 1");
  expect(frame).toContain("out 4");
  expect(frame).not.toContain("out 5");
  expect(frame).toContain("… +3 more lines");
});

test("a folded call draws nothing", async () => {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <EventRow event={toolEvent(EventKind.ToolCall, editPayload)} folded />
      </ThemeProvider>
    ),
    { width: 60, height: 4 },
  );
  try {
    await setup.renderOnce();
    expect(setup.captureCharFrame().trim()).toBe("");
  } finally {
    setup.renderer.destroy();
  }
});

test("a tool result with the same payload does not repeat the diff", async () => {
  const frame = await frameOf(toolEvent(EventKind.ToolResult, editPayload));
  expect(frame).not.toContain("src/a.ts");
  expect(frame).not.toContain("newValue");
});

test("a non-edit tool call renders only its preview line", async () => {
  const frame = await frameOf(
    toolEvent(EventKind.ToolCall, {
      type: "tool_use",
      name: "Bash",
      input: { command: "ls" },
    }),
  );
  expect(frame).toContain("$ ls");
  expect(frame).not.toContain("+0 -0");
});

test("a shell call shows its glyph, name and the head of a long command", async () => {
  const command = Array.from({ length: 6 }, (_, i) => `step ${i + 1}`).join("\n");
  const frame = await frameOf({
    ...toolEvent(EventKind.ToolCall, { type: "tool_use", name: "Bash", input: { command } }),
    content: "Bash",
  });
  expect(frame).toContain("$ step 1");
  expect(frame).not.toContain("Bash");
  expect(frame).toContain("step 4");
  expect(frame).not.toContain("step 5");
  expect(frame).toContain("… +2 more lines");
});
