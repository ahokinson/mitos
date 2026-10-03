import type { CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";

export const annotationCommands: Record<string, CommandEntry> = {
  note: {
    usage: "<text>",
    summary: "Attach a note to the selected thread",
    run: async (_args, argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(null, FeedbackTone.Error, "No thread selected.");
        return;
      }
      if (argLine === "") {
        ctx.note(thread.id, FeedbackTone.Error, "Usage: /note <text>");
        return;
      }
      try {
        await ctx.mutations.noteThread(thread.id, { note: argLine });
        ctx.events.sync();
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  decision: {
    usage: "<text>",
    summary: "Record a decision on the selected thread",
    run: async (_args, argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(null, FeedbackTone.Error, "No thread selected.");
        return;
      }
      if (argLine === "") {
        ctx.note(thread.id, FeedbackTone.Error, "Usage: /decision <text>");
        return;
      }
      try {
        await ctx.mutations.noteThread(thread.id, { decisions: [argLine] });
        ctx.events.sync();
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  question: {
    usage: "<text>",
    summary: "Record an open question on the selected thread",
    run: async (_args, argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(null, FeedbackTone.Error, "No thread selected.");
        return;
      }
      if (argLine === "") {
        ctx.note(thread.id, FeedbackTone.Error, "Usage: /question <text>");
        return;
      }
      try {
        await ctx.mutations.noteThread(thread.id, { questions: [argLine] });
        ctx.events.sync();
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },
};
