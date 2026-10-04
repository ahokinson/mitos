import type { CommandContext, CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import {
  Decision,
  type HarnessRequest,
  type Reply,
  RequestKind,
} from "@session/requests.ts";
import type { Thread } from "@session/threads.ts";

type PendingTarget = { thread: Thread; request: HarnessRequest };

function oldestPending(ctx: CommandContext): PendingTarget | null {
  const thread = ctx.selected();
  if (!thread) {
    ctx.note(null, FeedbackTone.Error, "No thread selected.");
    return null;
  }
  const request = ctx.pendingRequests()[0];
  if (!request) {
    ctx.note(
      thread.id,
      FeedbackTone.Error,
      "Nothing is waiting for an answer.",
    );
    return null;
  }
  return { thread, request };
}

async function submitAnswer(
  target: PendingTarget,
  reply: Reply,
  ctx: CommandContext,
): Promise<void> {
  try {
    await ctx.mutations.answerRequest(
      target.thread.id,
      target.request.id,
      reply,
    );
    ctx.refresh();
  } catch (cause) {
    ctx.note(target.thread.id, FeedbackTone.Error, describeFailure(cause));
  }
}

async function decide(
  decision: Decision,
  text: string,
  ctx: CommandContext,
): Promise<void> {
  const target = oldestPending(ctx);
  if (!target) return;
  if (target.request.kind === RequestKind.Question) {
    ctx.note(
      target.thread.id,
      FeedbackTone.Error,
      "That is a question; use /answer <text>.",
    );
    return;
  }
  await submitAnswer(target, { decision, ...(text ? { text } : {}) }, ctx);
}

export const requestCommands: Record<string, CommandEntry> = {
  approve: {
    usage: "",
    summary: "Approve the oldest pending harness request (permission or plan)",
    run: (_args, _argLine, ctx) => decide(Decision.Approve, "", ctx),
  },

  deny: {
    usage: "[reason]",
    summary: "Deny the oldest pending harness request, with optional feedback",
    run: (_args, argLine, ctx) => decide(Decision.Deny, argLine, ctx),
  },

  answer: {
    usage: "<text>",
    summary: "Answer the oldest pending harness question",
    run: async (_args, argLine, ctx) => {
      const target = oldestPending(ctx);
      if (!target) return;
      if (target.request.kind !== RequestKind.Question) {
        ctx.note(
          target.thread.id,
          FeedbackTone.Error,
          `The pending request is a ${target.request.kind}; use /approve or /deny.`,
        );
        return;
      }
      if (argLine === "") {
        ctx.note(target.thread.id, FeedbackTone.Error, "Usage: /answer <text>");
        return;
      }
      await submitAnswer(target, { text: argLine }, ctx);
    },
  },
};
