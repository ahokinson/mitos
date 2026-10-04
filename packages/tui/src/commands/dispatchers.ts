import type { CommandContext, CommandEntry } from "@commands/contexts.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import type { ParsedCommand } from "@commands/parsers.ts";
import { COMMANDS } from "@commands/registries.ts";

export async function dispatchCommand(
  command: ParsedCommand,
  ctx: CommandContext,
  commands: Readonly<Record<string, CommandEntry>> = COMMANDS,
): Promise<void> {
  const entry = commands[command.name];
  if (!entry) {
    ctx.note(
      ctx.selected()?.id ?? null,
      FeedbackTone.Error,
      `Unknown command "/${command.name}". Run /help to see available commands.`,
    );
    return;
  }
  await entry.run(command.args, command.argLine, ctx);
}
