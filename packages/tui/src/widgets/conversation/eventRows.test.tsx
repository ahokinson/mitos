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

test("a file-edit tool call renders its path, counts and diff lines", async () => {
  const frame = await frameOf(toolEvent(EventKind.ToolCall, editPayload));
  expect(frame).toContain("src/a.ts  +1 -1");
  expect(frame).toContain("oldValue");
  expect(frame).toContain("newValue");
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
  expect(frame).toContain("Edit");
  expect(frame).not.toContain("+0 -0");
});
