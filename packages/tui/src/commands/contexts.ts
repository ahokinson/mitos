import type { FeedbackTone } from "@commands/feedbackLines.ts";
import type { ThreadEventsState } from "@session/events.ts";
import type {
  abortThread,
  answerRequest,
  archiveThread,
  compactThread,
  deleteThread,
  diffThread,
  forkThread,
  hooksInit,
  hooksStatus,
  listAgents,
  listModels,
  noteThread,
  reassignHarness,
  runCommand,
  setAgent,
  setMode,
  setModel,
  shareThread,
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
    abortThread: typeof abortThread;
    forkThread: typeof forkThread;
    shareThread: typeof shareThread;
    diffThread: typeof diffThread;
    setAgent: typeof setAgent;
    setModel: typeof setModel;
    listAgents: typeof listAgents;
    listModels: typeof listModels;
    runCommand: typeof runCommand;
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
