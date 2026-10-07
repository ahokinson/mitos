import { expect, test } from "bun:test";
import {
  createFeedbackState,
  FeedbackTone,
  FeedItemKind,
  mergeFeed,
} from "@commands/feedbackLines.ts";
import { EventKind, type ThreadEvent } from "@session/events.ts";
import { createRoot } from "solid-js";

function event(
  overrides: Partial<ThreadEvent> & { created_at: string },
): ThreadEvent {
  return {
    thread_id: "t1",
    seq: 1,
    turn_id: null,
    harness: "claude",
    kind: EventKind.UserMessage,
    role: "user",
    content: "hi",
    payload: null,
    ...overrides,
  };
}

/** `createFeedbackState` uses `createSignal` internally — give it an owning
 * root like `picker.test.ts` used to for the old picker state. */
function withFeedback<R>(
  run: (feedback: ReturnType<typeof createFeedbackState>) => R,
): R {
  let result!: R;
  const dispose = createRoot((dispose) => {
    result = run(createFeedbackState());
    return dispose;
  });
  dispose();
  return result;
}

test("push appends lines with monotonically increasing ids", () => {
  withFeedback((feedback) => {
    feedback.push({ threadId: null, tone: FeedbackTone.Info, lines: "first" });
    feedback.push({
      threadId: null,
      tone: FeedbackTone.Error,
      lines: "second",
    });
    const [first, second] = feedback.lines();
    if (!first || !second) throw new Error("expected two feedback lines");
    expect(first.id).toBeLessThan(second.id);
  });
});

test("push accepts a single string or multiple lines", () => {
  withFeedback((feedback) => {
    feedback.push({
      threadId: "t1",
      tone: FeedbackTone.Info,
      lines: "one line",
    });
    feedback.push({
      threadId: "t1",
      tone: FeedbackTone.Info,
      lines: ["a", "b"],
    });
    const [solo, multi] = feedback.lines();
    expect(solo?.lines).toEqual(["one line"]);
    expect(multi?.lines).toEqual(["a", "b"]);
  });
});

test("mergeFeed interleaves real events and feedback lines by timestamp", () => {
  const events = [
    event({ seq: 1, created_at: "2026-01-01T00:00:00.000Z" }),
    event({ seq: 2, created_at: "2026-01-01T00:00:02.000Z" }),
  ];
  const feedbackLines = [
    {
      id: 1,
      threadId: "t1",
      tone: FeedbackTone.Info,
      lines: ["mid"],
      createdAt: Date.parse("2026-01-01T00:00:01.000Z"),
    },
  ];
  const merged = mergeFeed(events, feedbackLines, "t1");
  expect(merged.map((item) => item.kind)).toEqual([
    FeedItemKind.Event,
    FeedItemKind.Feedback,
    FeedItemKind.Event,
  ]);
});

test("mergeFeed returns the same item objects on every call", () => {
  const events = [event({ seq: 1, created_at: "2026-01-01T00:00:00.000Z" })];
  const feedbackLines = [
    {
      id: 1,
      threadId: "t1",
      tone: FeedbackTone.Info,
      lines: ["x"],
      createdAt: Date.parse("2026-01-01T00:00:01.000Z"),
    },
  ];
  const first = mergeFeed(events, feedbackLines, "t1");
  const second = mergeFeed(events, feedbackLines, "t1");
  expect(second[0]).toBe(first[0]);
  expect(second[1]).toBe(first[1]);
});

test("mergeFeed filters feedback lines down to the current thread context, including null", () => {
  const feedbackLines = [
    {
      id: 1,
      threadId: "t1",
      tone: FeedbackTone.Info,
      lines: ["for t1"],
      createdAt: 1,
    },
    {
      id: 2,
      threadId: null,
      tone: FeedbackTone.Info,
      lines: ["global"],
      createdAt: 2,
    },
    {
      id: 3,
      threadId: "t2",
      tone: FeedbackTone.Info,
      lines: ["for t2"],
      createdAt: 3,
    },
  ];
  expect(
    mergeFeed([], feedbackLines, "t1").map((item) =>
      item.kind === FeedItemKind.Feedback ? item.feedback.id : null,
    ),
  ).toEqual([1]);
  expect(
    mergeFeed([], feedbackLines, null).map((item) =>
      item.kind === FeedItemKind.Feedback ? item.feedback.id : null,
    ),
  ).toEqual([2]);
  expect(
    mergeFeed([], feedbackLines, "t2").map((item) =>
      item.kind === FeedItemKind.Feedback ? item.feedback.id : null,
    ),
  ).toEqual([3]);
});
