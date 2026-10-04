import type { CommandEntry } from "@commands/contexts.ts";
import { describeFailure } from "@commands/failures.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { formatThreadLine, pickThread } from "@commands/lookups.ts";
import {
  detectInstalledHarnesses,
  type KnownHarness,
} from "@harness/harnesses.ts";
import { CompactMode, type Thread } from "@session/threads.ts";

export const threadCommands: Record<string, CommandEntry> = {
  new: {
    usage: "[harness]",
    summary: "Start a new thread draft, optionally pinning its harness",
    run: (args, _argLine, ctx) => {
      const requested = args[0]?.toLowerCase();
      if (requested === undefined) {
        ctx.startNewThread(ctx.defaultHarness());
        return;
      }
      const installed = detectInstalledHarnesses();
      if (!installed.includes(requested as KnownHarness)) {
        ctx.note(
          ctx.selected()?.id ?? null,
          FeedbackTone.Error,
          `Unknown or uninstalled harness "${requested}". Installed: ${installed.join(", ") || "none"}.`,
        );
        return;
      }
      ctx.startNewThread(requested);
    },
  },

  harness: {
    usage: "<name>",
    summary: "Reassign the selected thread's harness",
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
      const requested = args[0]?.toLowerCase();
      if (!requested) {
        ctx.note(thread.id, FeedbackTone.Error, "Usage: /harness <name>");
        return;
      }
      const installed = detectInstalledHarnesses();
      if (!installed.includes(requested as KnownHarness)) {
        ctx.note(
          thread.id,
          FeedbackTone.Error,
          `Unknown or uninstalled harness "${requested}". Installed: ${installed.join(", ") || "none"}.`,
        );
        return;
      }
      try {
        await ctx.mutations.reassignHarness(thread.id, requested);
        ctx.setCurrentHarness(requested);
        ctx.refresh();
        ctx.note(thread.id, FeedbackTone.Info, `Reassigned to ${requested}.`);
      } catch (cause) {
        ctx.note(thread.id, FeedbackTone.Error, describeFailure(cause));
      }
    },
  },

  compact: {
    usage: "[mechanical|intelligent]",
    summary: "Restart the selected thread in a fresh context, trimmed or summarized",
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

  threads: {
    usage: "",
    summary: "List this workspace's threads",
    run: (_args, _argLine, ctx) => {
      const currentId = ctx.selected()?.id ?? null;
      const threads = ctx.list.threads();
      if (threads.length === 0) {
        ctx.note(
          currentId,
          FeedbackTone.Info,
          "No threads yet. Run /new <harness> to start one.",
        );
        return;
      }
      ctx.note(
        currentId,
        FeedbackTone.Info,
        threads.map((thread) => formatThreadLine(thread, currentId)),
      );
    },
  },

  resume: {
    usage: "<query>",
    summary: "Switch to a thread by id prefix, harness, or opening message",
    run: (_args, argLine, ctx) => {
      const currentId = ctx.selected()?.id ?? null;
      if (argLine === "") {
        ctx.note(
          currentId,
          FeedbackTone.Error,
          "Usage: /resume <query>. Run /threads to browse everything.",
        );
        return;
      }
      const thread = pickThread(ctx, argLine, currentId);
      if (!thread) return;
      ctx.setSelected(thread);
      ctx.setCurrentHarness(thread.active_harness);
      ctx.events.sync();
      ctx.bumpUsage();
      ctx.note(
        thread.id,
        FeedbackTone.Info,
        `Resumed ${thread.id.slice(0, 8)} (${thread.active_harness ?? "unassigned"}).`,
      );
    },
  },

  archive: {
    usage: "[query]",
    summary:
      "Archive a thread (keeps full history); defaults to the selected thread",
    run: async (_args, argLine, ctx) => {
      const current = ctx.selected();
      let target: Thread;
      if (argLine === "") {
        if (!current) {
          ctx.note(
            null,
            FeedbackTone.Error,
            "No thread selected. Usage: /archive [query].",
          );
          return;
        }
        target = current;
      } else {
        const picked = pickThread(ctx, argLine, current?.id ?? null);
        if (!picked) return;
        target = picked;
      }
      try {
        await ctx.mutations.archiveThread(target.id);
        const isCurrent = target.id === current?.id;
        if (isCurrent) ctx.setSelected(null);
        ctx.refresh();
        ctx.note(
          isCurrent ? null : (current?.id ?? null),
          FeedbackTone.Info,
          `Archived ${target.id.slice(0, 8)}.`,
        );
      } catch (cause) {
        ctx.note(
          current?.id ?? null,
          FeedbackTone.Error,
          describeFailure(cause),
        );
      }
    },
  },

  delete: {
    usage: "<query> | confirm <id> | cancel",
    summary:
      "Permanently delete a thread (irreversible) — requires confirmation",
    run: async (args, argLine, ctx) => {
      const current = ctx.selected();
      const sub = args[0]?.toLowerCase();

      if (sub === "cancel") {
        ctx.setPendingDelete(null);
        ctx.note(current?.id ?? null, FeedbackTone.Info, "Delete cancelled.");
        return;
      }

      if (sub === "confirm") {
        const prefix = args[1]?.toLowerCase();
        const pending = ctx.pendingDelete();
        if (!pending) {
          ctx.note(
            current?.id ?? null,
            FeedbackTone.Error,
            "No delete is pending. Run /delete <query> first.",
          );
          return;
        }
        if (!prefix || !pending.threadId.toLowerCase().startsWith(prefix)) {
          ctx.note(
            current?.id ?? null,
            FeedbackTone.Error,
            `That id doesn't match the pending delete (${pending.threadId.slice(0, 8)}). Run "/delete confirm ${pending.threadId.slice(0, 8)}", or /delete cancel.`,
          );
          return;
        }
        try {
          await ctx.mutations.deleteThread(pending.threadId);
          const isCurrent = pending.threadId === current?.id;
          if (isCurrent) ctx.setSelected(null);
          ctx.setPendingDelete(null);
          ctx.refresh();
          ctx.note(
            isCurrent ? null : (current?.id ?? null),
            FeedbackTone.Info,
            `Deleted ${pending.threadId.slice(0, 8)}.`,
          );
        } catch (cause) {
          ctx.note(
            current?.id ?? null,
            FeedbackTone.Error,
            describeFailure(cause),
          );
        }
        return;
      }

      if (argLine === "") {
        ctx.note(
          current?.id ?? null,
          FeedbackTone.Error,
          "Usage: /delete <query>, then /delete confirm <id8>.",
        );
        return;
      }
      const target = pickThread(ctx, argLine, current?.id ?? null);
      if (!target) return;
      ctx.setPendingDelete({
        threadId: target.id,
        label: target.opening_message ?? target.id,
      });
      ctx.note(current?.id ?? null, FeedbackTone.Info, [
        `This permanently deletes thread ${target.id.slice(0, 8)} (${target.active_harness ?? "unassigned"}) and cannot be undone.`,
        `Run "/delete confirm ${target.id.slice(0, 8)}" to proceed, or "/delete cancel" to back out.`,
      ]);
    },
  },
};
