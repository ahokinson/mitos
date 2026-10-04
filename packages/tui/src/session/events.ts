import { syncEvents } from "@database/views.ts";
import type { FileDiff } from "@session/diffs.ts";
import {
  describeRequest,
  type RequestKind,
  RequestStatus,
  requestHint,
} from "@session/requests.ts";
import type { ToolFacts } from "@session/tools.ts";
import { Tone } from "@theme/themes.ts";
import { createEffect, createSignal, on } from "solid-js";

export enum EventKind {
  UserMessage = "user_message",
  AssistantMessage = "assistant_message",
  AssistantDelta = "assistant_delta",
  ToolCall = "tool_call",
  ToolResult = "tool_result",
  Status = "status",
  Usage = "usage",
  Error = "error",
  Note = "note",
  Decision = "decision",
  Question = "question",
  RequestOpened = "request_opened",
  RequestAnswered = "request_answered",
  ModeChanged = "mode_changed",
  ThreadCreated = "thread_created",
  HarnessBound = "harness_bound",
  HarnessUnbound = "harness_unbound",
  HandoffCarryover = "handoff_carryover",
  Compaction = "compaction",
}

/** Mirrors `crates/mitos/src/domain/events.rs::ThreadEvent`, snake_case to match
 * the untransformed serde JSON on the wire. */
export type ThreadEvent = {
  thread_id: string;
  seq: number;
  turn_id: string | null;
  /** Free-text, same reasoning as `Thread.active_harness` in `models/threads.ts`. */
  harness: string | null;
  kind: EventKind;
  role: string | null;
  content: string | null;
  payload: unknown | null;
  created_at: string;
  /** Computed by `thread sync`; empty except on tool events. */
  diffs?: FileDiff[];
  /** Computed by `thread sync`; present only on tool events. */
  tool?: ToolFacts;
};

export type ThreadEventsState = {
  events: () => ThreadEvent[];
  sync: () => void;
};

export type EventPresentation = {
  glyph: string;
  tone: Tone;
};

/** Tracks one thread's event log, polling for events past the last seen
 * seq; resets when `threadId` changes. */
export function createThreadEventsState(
  threadId: () => string | null,
): ThreadEventsState {
  const [events, setEvents] = createSignal<ThreadEvent[]>([]);
  let lastSeq = 0;

  createEffect(
    on(threadId, () => {
      setEvents([]);
      lastSeq = 0;
    }),
  );

  function sync(): void {
    const id = threadId();
    if (!id) return;
    const fresh = syncEvents(id, lastSeq);
    const last = fresh.at(-1);
    if (!last) return;
    lastSeq = last.seq;
    setEvents((current) => [...current, ...fresh]);
  }

  return { events, sync };
}

/** Whether an event is worth a line in the conversation view. Usage/status
 * bookkeeping and structural markers are mitos's own accounting. */
export function isDisplayEvent(event: ThreadEvent): boolean {
  switch (event.kind) {
    case EventKind.UserMessage:
    case EventKind.AssistantMessage:
    case EventKind.AssistantDelta:
    case EventKind.ToolCall:
    case EventKind.ToolResult:
    case EventKind.Error:
    case EventKind.Note:
    case EventKind.Decision:
    case EventKind.Question:
    case EventKind.RequestOpened:
    case EventKind.RequestAnswered:
    case EventKind.ModeChanged:
      return true;
    default:
      return false;
  }
}

function payloadRecord(event: ThreadEvent): Record<string, unknown> {
  return typeof event.payload === "object" && event.payload !== null
    ? (event.payload as Record<string, unknown>)
    : {};
}

/** Text for a display row: events that carry no `content` of their own
 * (requests, mode changes) get theirs derived from the payload. */
export function eventText(event: ThreadEvent): string {
  const payload = payloadRecord(event);
  switch (event.kind) {
    case EventKind.RequestOpened: {
      const kind = payload.kind as RequestKind;
      return `${describeRequest(kind, payload.request)}\n${requestHint(kind)}`;
    }
    case EventKind.RequestAnswered:
      return payload.status === RequestStatus.Cancelled
        ? "Request cancelled."
        : "Request answered.";
    case EventKind.ModeChanged:
      return `Mode: ${String(payload.mode)}`;
    default:
      return event.content ?? "";
  }
}

/** Collapses consecutive `assistant_delta` events of a turn into one growing
 * entry; a trailing `assistant_message` replaces the accumulated text.
 * Expects `isDisplayEvent`-filtered input. */
export function accumulateAssistantDeltas(
  events: readonly ThreadEvent[],
): ThreadEvent[] {
  const merged: ThreadEvent[] = [];
  for (const event of events) {
    const last = merged[merged.length - 1];
    const isStreamingAssistant =
      event.kind === EventKind.AssistantDelta ||
      event.kind === EventKind.AssistantMessage;
    const continuesRun =
      isStreamingAssistant &&
      last !== undefined &&
      (last.kind === EventKind.AssistantDelta ||
        last.kind === EventKind.AssistantMessage) &&
      last.turn_id !== null &&
      last.turn_id === event.turn_id;
    if (continuesRun && last !== undefined) {
      merged[merged.length - 1] = {
        ...event,
        content:
          event.kind === EventKind.AssistantMessage
            ? event.content
            : `${last.content ?? ""}${event.content ?? ""}`,
      };
      continue;
    }
    merged.push(event);
  }
  return merged;
}

export function eventPresentation(kind: EventKind): EventPresentation {
  switch (kind) {
    case EventKind.UserMessage:
      return { glyph: "›", tone: Tone.Accent };
    case EventKind.AssistantMessage:
    case EventKind.AssistantDelta:
      return { glyph: "✦", tone: Tone.Success };
    case EventKind.ToolCall:
      return { glyph: "◌", tone: Tone.Warning };
    case EventKind.ToolResult:
      return { glyph: "↳", tone: Tone.Muted };
    case EventKind.Error:
      return { glyph: "×", tone: Tone.Error };
    case EventKind.Note:
      return { glyph: "·", tone: Tone.Dim };
    case EventKind.Decision:
      return { glyph: "◆", tone: Tone.Accent };
    case EventKind.Question:
    case EventKind.RequestOpened:
      return { glyph: "?", tone: Tone.Warning };
    case EventKind.RequestAnswered:
      return { glyph: "✓", tone: Tone.Dim };
    case EventKind.ModeChanged:
      return { glyph: "◇", tone: Tone.Accent };
    default:
      return { glyph: "·", tone: Tone.Dim };
  }
}

/** Keep tool activity scannable without claiming a result succeeded. */
export function toolPreview(content: string | null, fallback: string): string {
  const full = (content ?? "").trim();
  if (!full) return fallback;
  const firstLine = (full.split(/\r?\n/, 1)[0] ?? "")
    .replace(/\s+/g, " ")
    .trim();
  if (!firstLine) return fallback;
  const limit = 120;
  return full.length > firstLine.length || firstLine.length > limit
    ? `${firstLine.slice(0, limit).trimEnd()}…`
    : firstLine;
}
