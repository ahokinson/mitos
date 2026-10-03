export type ParsedCommand = {
  name: string;
  args: readonly string[];
  argLine: string;
};

export enum ParsedKind {
  Chat = "chat",
  Command = "command",
}

export type ParsedInput =
  | { kind: ParsedKind.Chat; text: string }
  | { kind: ParsedKind.Command; command: ParsedCommand };

/** A leading `/` runs a command; `//` escapes to a literal chat message
 * starting with `/` (standard chat-app convention). Everything else is
 * chat text, unchanged. */
export function parseCommandInput(raw: string): ParsedInput {
  if (raw.startsWith("//"))
    return { kind: ParsedKind.Chat, text: raw.slice(1) };
  if (!raw.startsWith("/")) return { kind: ParsedKind.Chat, text: raw };

  const rest = raw.slice(1).trim();
  const firstSpace = rest.search(/\s/);
  const name = (
    firstSpace === -1 ? rest : rest.slice(0, firstSpace)
  ).toLowerCase();
  const argLine = firstSpace === -1 ? "" : rest.slice(firstSpace + 1).trim();
  const args = argLine === "" ? [] : argLine.split(/\s+/);
  return { kind: ParsedKind.Command, command: { name, args, argLine } };
}
