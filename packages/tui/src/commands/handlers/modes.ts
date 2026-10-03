import type { CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { ThreadMode } from "@session/threads.ts";

export const modeCommands: Record<string, CommandEntry> = {
  mode: {
    usage: "[plan|build]",
    summary:
      "Show or set the selected thread's mode; applies from the next turn",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(null, FeedbackTone.Error, "No thread selected.");
        return;
      }
      const requested = args[0]?.toLowerCase();
      if (requested === undefined) {
        ctx.note(thread.id, FeedbackTone.Info, `Mode: ${thread.mode}.`);
        return;
      }
      if (requested !== ThreadMode.Plan && requested !== ThreadMode.Build) {
        ctx.note(thread.id, FeedbackTone.Error, "Usage: /mode [plan|build]");
        return;
      }
      try {
        await ctx.mutations.setMode(thread.id, requested);
        ctx.refresh();
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },
};
