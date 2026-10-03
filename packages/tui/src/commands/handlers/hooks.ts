import type { CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { initLines, statusLines } from "@commands/hookReports.ts";

const USAGE = "Usage: /hooks [status | init [harness…]]";

export const hookCommands: Record<string, CommandEntry> = {
  hooks: {
    usage: "[status | init [harness…]]",
    summary:
      "Show whether each harness's Mitos hook is installed and approved, or install them",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected()?.id ?? null;
      const [action = "status", ...harnesses] = args;
      try {
        if (action === "status" && harnesses.length === 0) {
          ctx.note(
            thread,
            FeedbackTone.Info,
            statusLines(await ctx.mutations.hooksStatus()),
          );
        } else if (action === "init") {
          ctx.note(
            thread,
            FeedbackTone.Info,
            initLines(await ctx.mutations.hooksInit(harnesses)),
          );
        } else {
          ctx.note(thread, FeedbackTone.Error, USAGE);
        }
      } catch (cause) {
        ctx.note(thread, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },
};
