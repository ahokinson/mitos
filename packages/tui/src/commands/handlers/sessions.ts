import type { CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { CompactMode } from "@session/threads.ts";

export const sessionCommands: Record<string, CommandEntry> = {
  compact: {
    usage: "[mechanical|intelligent]",
    summary:
      "Restart the selected thread in a fresh context, trimmed or summarized",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      const requested = args[0]?.toLowerCase() ?? CompactMode.Mechanical;
      if (
        requested !== CompactMode.Mechanical &&
        requested !== CompactMode.Intelligent
      ) {
        ctx.note(
          thread.id,
          FeedbackTone.Error,
          "Usage: /compact [mechanical|intelligent]",
        );
        return;
      }
      try {
        await ctx.mutations.compactThread(thread.id, requested);
        ctx.refresh();
        ctx.note(thread.id, FeedbackTone.Info, `Compacted (${requested}).`);
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  abort: {
    usage: "",
    summary: "Ask OpenCode to stop the selected thread's running turn",
    run: async (_args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        await ctx.mutations.abortThread(thread.id);
        ctx.note(
          thread.id,
          FeedbackTone.Info,
          "Asked OpenCode to stop the running turn.",
        );
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  fork: {
    usage: "",
    summary: "Fork the selected OpenCode thread's session into a new thread",
    run: async (_args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        const forked = await ctx.mutations.forkThread(thread.id);
        ctx.refresh();
        ctx.setSelected(forked);
        ctx.setCurrentHarness(forked.active_harness);
        ctx.events.sync();
        ctx.note(
          forked.id,
          FeedbackTone.Info,
          `Forked ${forked.id.slice(0, 8)} from ${thread.id.slice(0, 8)}.`,
        );
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  share: {
    usage: "",
    summary:
      "Publish the selected OpenCode thread's session and print its link",
    run: async (_args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        const url = await ctx.mutations.shareThread(thread.id);
        ctx.note(
          thread.id,
          FeedbackTone.Info,
          url === "" ? "OpenCode returned no link." : url,
        );
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  diff: {
    usage: "",
    summary: "Show the files the selected OpenCode thread's session changed",
    run: async (_args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        const lines = await ctx.mutations.diffThread(thread.id);
        ctx.note(
          thread.id,
          FeedbackTone.Info,
          lines.length === 0
            ? "No file changes recorded for this session."
            : lines,
        );
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  agent: {
    usage: "[name]",
    summary:
      "Set the selected thread's agent (OpenCode); bare lists the server's agents",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        if (args.length === 0) {
          const lines = await ctx.mutations.listAgents(thread.id);
          ctx.note(
            thread.id,
            FeedbackTone.Info,
            lines.length === 0 ? "The server reported no agents." : lines,
          );
          return;
        }
        const name = args[0] ?? "";
        await ctx.mutations.setAgent(thread.id, name);
        ctx.refresh();
        ctx.note(thread.id, FeedbackTone.Info, `Agent set to ${name}.`);
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  model: {
    usage: "[provider/model]",
    summary:
      "Set the selected thread's model (OpenCode); bare lists the server's defaults",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      try {
        if (args.length === 0) {
          const lines = await ctx.mutations.listModels(thread.id);
          ctx.note(
            thread.id,
            FeedbackTone.Info,
            lines.length === 0
              ? "The server reported no default models."
              : lines,
          );
          return;
        }
        const reference = args[0] ?? "";
        await ctx.mutations.setModel(thread.id, reference);
        ctx.refresh();
        ctx.note(thread.id, FeedbackTone.Info, `Model set to ${reference}.`);
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  oc: {
    usage: "<command> [args...]",
    summary: "Run an OpenCode slash command in the selected thread's session",
    run: async (args, _argLine, ctx) => {
      const thread = ctx.selected();
      if (!thread) {
        ctx.note(
          null,
          FeedbackTone.Error,
          "No thread selected. Run /new <harness> to start one, or /resume <query> to pick one up.",
        );
        return;
      }
      const command = args[0];
      if (!command) {
        ctx.note(
          thread.id,
          FeedbackTone.Error,
          "Usage: /oc <command> [args...]",
        );
        return;
      }
      try {
        const text = await ctx.mutations.runCommand(
          thread.id,
          command.replace(/^\/+/, ""),
          args.slice(1),
        );
        ctx.note(
          thread.id,
          FeedbackTone.Info,
          text === "" ? "The command returned no text." : text,
        );
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },
};
