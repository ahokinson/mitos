import type { ThreadEvent } from "@session/events.ts";
import { createSignal } from "solid-js";

export enum FeedbackTone {
  Info = "info",
  Error = "error",
}

export enum FeedItemKind {
  Event = "event",
  Feedback = "feedback",
}

/** A command's output — a `/threads` listing, a confirmation, an error.
 * Never written to `thread_events`: `threadId` is a display tag used only
 * to filter at render time, not a foreign key into any table. */
export type FeedbackLine = {
  id: number;
  threadId: string | null;
  tone: FeedbackTone;
  lines: readonly string[];
  createdAt: number;
};

export type FeedbackState = {
  lines: () => readonly FeedbackLine[];
  push: (entry: {
    threadId: string | null;
    tone: FeedbackTone;
    lines: string | readonly string[];
  }) => void;
};

/** One flat, app-level log (not per-thread signals). Lines are never
 * deleted on a thread switch, only filtered out of view by `mergeFeed` —
 * switching back later brings them back in correct chronological order,
 * same as real events already behave. */
export function createFeedbackState(): FeedbackState {
  const [lines, setLines] = createSignal<FeedbackLine[]>([]);
  let nextId = 1;

  function push(entry: {
    threadId: string | null;
    tone: FeedbackTone;
    lines: string | readonly string[];
  }): void {
    const id = nextId++;
    const body = typeof entry.lines === "string" ? [entry.lines] : entry.lines;
    setLines((current) => [
      ...current,
      {
        id,
        threadId: entry.threadId,
        tone: entry.tone,
        lines: body,
        createdAt: Date.now(),
      },
    ]);
  }

  return { lines, push };
}

export type FeedItem =
  | { kind: FeedItemKind.Event; event: ThreadEvent }
  | { kind: FeedItemKind.Feedback; feedback: FeedbackLine };

/** Interleaves a thread's real events with command feedback by timestamp,
 * filtering feedback down to whichever thread context (including `null`,
 * meaning "no thread selected") is currently showing. */
export function mergeFeed(
  events: readonly ThreadEvent[],
  feedbackLines: readonly FeedbackLine[],
  currentThreadId: string | null,
): FeedItem[] {
  const scoped = feedbackLines.filter(
    (feedback) => feedback.threadId === currentThreadId,
  );
  const items: { item: FeedItem; at: number }[] = [
    ...events.map((event) => ({
      item: { kind: FeedItemKind.Event, event },
      at: Date.parse(event.created_at),
    })),
    ...scoped.map((feedback) => ({
      item: { kind: FeedItemKind.Feedback, feedback },
      at: feedback.createdAt,
    })),
  ];
  items.sort((a, b) => a.at - b.at);
  return items.map(({ item }) => item);
}
