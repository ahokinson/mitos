import { expect, test } from "bun:test";
import {
  accumulateAssistantDeltas,
  EventKind,
  eventPresentation,
  eventText,
  isDisplayEvent,
  type ThreadEvent,
  toolPreview,
} from "@session/events.ts";
import { Tone } from "@theme/themes.ts";

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

test("accumulateAssistantDeltas merges consecutive deltas sharing a turn_id", () => {
  const merged = accumulateAssistantDeltas([
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

test("accumulateAssistantDeltas replaces accumulated text with the trailing assistant_message", () => {
  const merged = accumulateAssistantDeltas([
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

test("accumulateAssistantDeltas keeps deltas from different turns separate", () => {
  const merged = accumulateAssistantDeltas([
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

test("accumulateAssistantDeltas never merges across a null turn_id", () => {
  const merged = accumulateAssistantDeltas([
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

test("accumulateAssistantDeltas doesn't merge across an interleaved event of another kind", () => {
  const merged = accumulateAssistantDeltas([
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

test("accumulateAssistantDeltas passes non-assistant events through unchanged", () => {
  const userEvent = event({ kind: EventKind.UserMessage, content: "hi" });
  const merged = accumulateAssistantDeltas([userEvent]);
  expect(merged[0]).toBe(userEvent);
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
