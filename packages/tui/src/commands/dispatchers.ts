import type { CommandContext } from "@commands/contexts.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import type { ParsedCommand } from "@commands/parsers.ts";
import { COMMANDS } from "@commands/registries.ts";

export async function dispatchCommand(
  command: ParsedCommand,
  ctx: CommandContext,
): Promise<void> {
  const entry = COMMANDS[command.name];
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
