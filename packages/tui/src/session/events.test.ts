import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  applyDisplayEvent,
  createThreadEventsState,
  EventKind,
  eventPresentation,
  eventText,
  isDisplayEvent,
  type ThreadEvent,
  toolPreview,
} from "@session/events.ts";
import { Tone } from "@theme/themes.ts";
import { createEffect, createRoot, createSignal } from "solid-js";
import { createStore } from "solid-js/store";

function event(
  partial: Partial<ThreadEvent> & Pick<ThreadEvent, "kind">,
): ThreadEvent {
  return {
    thread_id: "t1",
    seq: 1,
    turn_id: null,
    harness: "claude",
    role: null,
    content: null,
    payload: null,
    created_at: "2026-01-01T00:00:00Z",
    ...partial,
  };
}

function fold(events: ThreadEvent[]): ThreadEvent[] {
  const [rows, setRows] = createStore<ThreadEvent[]>([]);
  for (const next of events) applyDisplayEvent(rows, setRows, next);
  return rows;
}

test("applyDisplayEvent grows one row in place across a streaming turn", () => {
  const [rows, setRows] = createStore<ThreadEvent[]>([]);
  applyDisplayEvent(
    rows,
    setRows,
    event({ kind: EventKind.UserMessage, seq: 1, content: "hi" }),
  );
  applyDisplayEvent(
    rows,
    setRows,
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 2,
      content: "a",
    }),
  );
  const user = rows[0];
  const assistant = rows[1];
  applyDisplayEvent(
    rows,
    setRows,
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 3,
      content: "b",
    }),
  );
  applyDisplayEvent(
    rows,
    setRows,
    event({
      kind: EventKind.AssistantMessage,
      turn_id: "turn1",
      seq: 4,
      content: "ab!",
    }),
  );
  expect(rows).toHaveLength(2);
  expect(rows[0]).toBe(user);
  expect(rows[1]).toBe(assistant);
  expect(rows[1]?.content).toBe("ab!");
});

test("applyDisplayEvent notifies only the fields that changed", () => {
  const [rows, setRows] = createStore<ThreadEvent[]>([]);
  applyDisplayEvent(
    rows,
    setRows,
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 1,
      content: "a",
    }),
  );
  const seen = { content: 0, kind: 0 };
  const dispose = createRoot((dispose) => {
    createEffect(() => {
      void rows[0]?.content;
      seen.content++;
    });
    createEffect(() => {
      void rows[0]?.kind;
      seen.kind++;
    });
    return dispose;
  });
  expect(seen).toEqual({ content: 1, kind: 1 });
  applyDisplayEvent(
    rows,
    setRows,
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 2,
      content: "b",
    }),
  );
  expect(seen).toEqual({ content: 2, kind: 1 });
  dispose();
});

test("applyDisplayEvent drops bookkeeping events", () => {
  expect(fold([event({ kind: EventKind.Usage })])).toHaveLength(0);
});

test("applyDisplayEventmerges consecutive deltas sharing a turn_id", () => {
  const merged = fold([
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 1,
      content: "Hel",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 2,
      content: "lo, ",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 3,
      content: "world!",
    }),
  ]);
  expect(merged).toHaveLength(1);
  expect(merged[0]?.content).toBe("Hello, world!");
  expect(merged[0]?.seq).toBe(3);
});

test("applyDisplayEventreplaces accumulated text with the trailing assistant_message", () => {
  const merged = fold([
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 1,
      content: "Hel",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 2,
      content: "lo",
    }),
    event({
      kind: EventKind.AssistantMessage,
      turn_id: "turn1",
      seq: 3,
      content: "Hello, world!",
    }),
  ]);
  expect(merged).toHaveLength(1);
  expect(merged[0]?.content).toBe("Hello, world!");
  expect(merged[0]?.kind).toBe(EventKind.AssistantMessage);
});

test("applyDisplayEventkeeps deltas from different turns separate", () => {
  const merged = fold([
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 1,
      content: "first",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn2",
      seq: 2,
      content: "second",
    }),
  ]);
  expect(merged).toHaveLength(2);
  expect(merged[0]?.content).toBe("first");
  expect(merged[1]?.content).toBe("second");
});

test("applyDisplayEventnever merges across a null turn_id", () => {
  const merged = fold([
    event({
      kind: EventKind.AssistantDelta,
      turn_id: null,
      seq: 1,
      content: "a",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: null,
      seq: 2,
      content: "b",
    }),
  ]);
  expect(merged).toHaveLength(2);
});

test("applyDisplayEventdoesn't merge across an interleaved event of another kind", () => {
  const merged = fold([
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 1,
      content: "a",
    }),
    event({
      kind: EventKind.ToolCall,
      turn_id: "turn1",
      seq: 2,
      content: "tool",
    }),
    event({
      kind: EventKind.AssistantDelta,
      turn_id: "turn1",
      seq: 3,
      content: "b",
    }),
  ]);
  expect(merged).toHaveLength(3);
  expect(merged.map((e) => e.content)).toEqual(["a", "tool", "b"]);
});

test("applyDisplayEventpasses non-assistant events through unchanged", () => {
  const userEvent = event({ kind: EventKind.UserMessage, content: "hi" });
  const merged = fold([userEvent]);
  expect(merged[0]).toEqual(userEvent);
});

test("displayed events use distinct semantic glyphs and tones", () => {
  expect(eventPresentation(EventKind.UserMessage)).toEqual({
    glyph: "›",
    tone: Tone.Accent,
  });
  expect(eventPresentation(EventKind.AssistantMessage)).toEqual({
    glyph: "✦",
    tone: Tone.Success,
  });
  expect(eventPresentation(EventKind.ToolCall)).toEqual({
    glyph: "◌",
    tone: Tone.Warning,
  });
  expect(eventPresentation(EventKind.ToolResult)).toEqual({
    glyph: "↳",
    tone: Tone.Muted,
  });
  expect(eventPresentation(EventKind.Error)).toEqual({
    glyph: "×",
    tone: Tone.Error,
  });
  expect(eventPresentation(EventKind.Note)).toEqual({
    glyph: "·",
    tone: Tone.Dim,
  });
  expect(eventPresentation(EventKind.Decision)).toEqual({
    glyph: "◆",
    tone: Tone.Accent,
  });
  expect(eventPresentation(EventKind.Question)).toEqual({
    glyph: "?",
    tone: Tone.Warning,
  });
});

test("remaining event kinds present with their own glyphs", () => {
  expect(eventPresentation(EventKind.AssistantDelta).glyph).toBe("✦");
  expect(eventPresentation(EventKind.RequestOpened).glyph).toBe("?");
  expect(eventPresentation(EventKind.RequestAnswered)).toEqual({
    glyph: "✓",
    tone: Tone.Dim,
  });
  expect(eventPresentation(EventKind.ModeChanged)).toEqual({
    glyph: "◇",
    tone: Tone.Accent,
  });
  expect(eventPresentation(EventKind.Usage)).toEqual({
    glyph: "·",
    tone: Tone.Dim,
  });
});

test("bookkeeping events are not displayed", () => {
  for (const kind of [
    EventKind.Status,
    EventKind.Usage,
    EventKind.ThreadCreated,
    EventKind.HarnessBound,
    EventKind.HarnessUnbound,
    EventKind.HandoffCarryover,
    EventKind.Compaction,
  ])
    expect(isDisplayEvent(event({ kind }))).toBe(false);
  for (const kind of [
    EventKind.UserMessage,
    EventKind.AssistantMessage,
    EventKind.AssistantDelta,
    EventKind.ToolCall,
    EventKind.ToolResult,
    EventKind.Error,
    EventKind.Note,
    EventKind.Decision,
    EventKind.Question,
    EventKind.RequestOpened,
    EventKind.RequestAnswered,
    EventKind.ModeChanged,
  ])
    expect(isDisplayEvent(event({ kind }))).toBe(true);
});

test("event text falls back to content, then to nothing", () => {
  expect(eventText(event({ kind: EventKind.Note, content: "hi" }))).toBe("hi");
  expect(eventText(event({ kind: EventKind.Note }))).toBe("");
  expect(
    eventText(event({ kind: EventKind.RequestAnswered, payload: "text" })),
  ).toBe("Request answered.");
});

test("a missing tool result previews as the fallback", () => {
  expect(toolPreview(null, "started")).toBe("started");
});

describe("createThreadEventsState", () => {
  let dir: string;
  let savedCore: string | undefined;

  async function serve(events: ThreadEvent[]): Promise<void> {
    const path = join(dir, "core.sh");
    await writeFile(
      path,
      `#!/bin/sh\ncat <<'EOF'\n${JSON.stringify(events)}\nEOF\n`,
    );
    await chmod(path, 0o755);
    process.env.MITOS_CORE = path;
  }

  beforeEach(async () => {
    dir = join(tmpdir(), `mitos-events-test-${crypto.randomUUID()}`);
    await mkdir(dir, { recursive: true });
    savedCore = process.env.MITOS_CORE;
  });

  afterEach(async () => {
    if (savedCore === undefined) delete process.env.MITOS_CORE;
    else process.env.MITOS_CORE = savedCore;
    await rm(dir, { recursive: true, force: true });
  });

  test("sync appends fresh events and ignores empty polls", async () => {
    await serve([event({ kind: EventKind.Note, seq: 4 })]);
    await createRoot(async (dispose) => {
      const [id] = createSignal<string | null>("t1");
      const state = createThreadEventsState(id);
      await Promise.resolve();
      state.sync();
      expect(state.events().map((e) => e.seq)).toEqual([4]);
      await serve([]);
      state.sync();
      expect(state.events()).toHaveLength(1);
      dispose();
    });
  });

  test("sync does nothing without a thread", () => {
    createRoot((dispose) => {
      const [id] = createSignal<string | null>(null);
      const state = createThreadEventsState(id);
      state.sync();
      expect(state.events()).toEqual([]);
      dispose();
    });
  });

  test("switching threads clears the log", async () => {
    await serve([event({ kind: EventKind.Note, seq: 1 })]);
    await createRoot(async (dispose) => {
      const [id, setId] = createSignal<string | null>("t1");
      const state = createThreadEventsState(id);
      await Promise.resolve();
      state.sync();
      expect(state.events()).toHaveLength(1);
      setId("t2");
      await Promise.resolve();
      expect(state.events()).toEqual([]);
      dispose();
    });
  });
});

test("tool previews normalize and truncate the first line", () => {
  expect(toolPreview("  bun   test\npassed", "completed")).toBe("bun test…");
  expect(toolPreview("", "started")).toBe("started");
  expect(toolPreview("x".repeat(121), "completed")).toBe(`${"x".repeat(120)}…`);
});

test("request and mode events render derived text", () => {
  const opened = event({
    kind: EventKind.RequestOpened,
    payload: {
      request_id: "r1",
      kind: "permission",
      request: { title: "Bash rm -rf build" },
    },
  });
  expect(isDisplayEvent(opened)).toBe(true);
  expect(eventText(opened)).toContain("Permission: Bash rm -rf build");
  expect(eventText(opened)).toContain("/approve");
  expect(
    eventText(
      event({ kind: EventKind.ModeChanged, payload: { mode: "plan" } }),
    ),
  ).toBe("Mode: plan");
  expect(
    eventText(
      event({
        kind: EventKind.RequestAnswered,
        payload: { status: "cancelled" },
      }),
    ),
  ).toBe("Request cancelled.");
});
