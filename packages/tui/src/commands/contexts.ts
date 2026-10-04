import type { FeedbackTone } from "@commands/feedbackLines.ts";
import type { ThreadEventsState } from "@session/events.ts";
import type {
  answerRequest,
  archiveThread,
  compactThread,
  deleteThread,
  hooksInit,
  hooksStatus,
  noteThread,
  reassignHarness,
  setMode,
} from "@session/mutations.ts";
import type { HarnessRequest } from "@session/requests.ts";
import type { Thread, ThreadListState } from "@session/threads.ts";

export type PendingDelete = { threadId: string; label: string };

export type CommandContext = {
  list: ThreadListState;
  selected: () => Thread | null;
  setSelected: (thread: Thread | null) => void;
  setCurrentHarness: (harness: string | null) => void;
  defaultHarness: () => string | null;
  startNewThread: (harness: string | null) => void;
  events: ThreadEventsState;
  pendingDelete: () => PendingDelete | null;
  setPendingDelete: (value: PendingDelete | null) => void;
  pendingRequests: () => readonly HarnessRequest[];
  refresh: () => void;
  bumpUsage: () => void;
  note: (
    threadId: string | null,
    tone: FeedbackTone,
    lines: string | readonly string[],
  ) => void;
  mutations: {
    reassignHarness: typeof reassignHarness;
    archiveThread: typeof archiveThread;
    compactThread: typeof compactThread;
    deleteThread: typeof deleteThread;
    noteThread: typeof noteThread;
    setMode: typeof setMode;
    answerRequest: typeof answerRequest;
    hooksStatus: typeof hooksStatus;
    hooksInit: typeof hooksInit;
  };
};

export type CommandEntry = {
  usage: string;
  summary: string;
  run: (
    args: readonly string[],
    argLine: string,
    ctx: CommandContext,
  ) => void | Promise<void>;
};
