import type { CommandEntry } from "@commands/contexts.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { annotationCommands } from "@commands/handlers/annotations.ts";
import { hookCommands } from "@commands/handlers/hooks.ts";
import { modeCommands } from "@commands/handlers/modes.ts";
import { requestCommands } from "@commands/handlers/requests.ts";
import { threadCommands } from "@commands/handlers/threads.ts";

export const COMMANDS: Record<string, CommandEntry> = {
  help: {
    usage: "",
    summary: "List available commands",
    run: (_args, _argLine, ctx) => {
      const lines = Object.entries(COMMANDS)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(
          ([name, entry]) =>
            `/${name}${entry.usage ? ` ${entry.usage}` : ""} — ${entry.summary}`,
        );
      lines.push(
        'Prefix a literal message that starts with "/" using "//" to send it as chat text.',
      );
      ctx.note(ctx.selected()?.id ?? null, FeedbackTone.Info, lines);
    },
  },

  ...threadCommands,
  ...annotationCommands,
  ...modeCommands,
  ...requestCommands,
  ...hookCommands,
};
