import { Tone } from "@theme/themes.ts";

export enum ToolKind {
  Shell = "shell",
  Read = "read",
  Edit = "edit",
  Write = "write",
  Search = "search",
  Fetch = "fetch",
  Agent = "agent",
  Todo = "todo",
  Other = "other",
}

/** Which of the payload's own shapes an event carries. */
export enum ToolShape {
  ToolUse = "tool_use",
  ToolResult = "tool_result",
  Other = "other",
}

/** Mirrors `crates/mitos/src/tools/mod.rs::ToolFacts`: what a tool event's
 * payload says, normalized across harnesses. */
export type ToolFacts = {
  kind: ToolKind;
  /** The tool's own name, when the harness reports one separately. */
  name: string | null;
  argument: string;
  /** The id that ties a call to its result. */
  tool_use_id: string | null;
  path: string | null;
  shape: ToolShape;
  /** A successful read or file edit, whose result adds nothing to its call. */
  quiet: boolean;
  patched: boolean;
};

type WithFacts = { tool?: ToolFacts };

export type ToolCall = {
  kind: ToolKind;
  name: string | null;
  /** Lines of the main argument, capped at `MAX_ARGUMENT_LINES`. */
  lines: string[];
  /** Lines of the argument left out of `lines`. */
  hidden: number;
};

const MAX_ARGUMENT_LINES = 4;
const MAX_LINE_LENGTH = 400;

function splitArgument(text: string): Pick<ToolCall, "lines" | "hidden"> {
  const trimmed = text.trim();
  const all = trimmed
    ? trimmed.split(/\r?\n/).map((line) => line.trimEnd())
    : [];
  const lines = all
    .slice(0, MAX_ARGUMENT_LINES)
    .map((line) =>
      line.length > MAX_LINE_LENGTH
        ? `${line.slice(0, MAX_LINE_LENGTH)}…`
        : line,
    );
  return { lines, hidden: all.length - lines.length };
}

export function shortenPath(path: string): string {
  const cwd = process.cwd();
  const home = process.env.HOME;
  if (path.startsWith(`${cwd}/`)) return path.slice(cwd.length + 1);
  if (home && path.startsWith(`${home}/`)) return `~${path.slice(home.length)}`;
  return path;
}

const PATH_KINDS: ReadonlySet<ToolKind> = new Set([
  ToolKind.Read,
  ToolKind.Edit,
  ToolKind.Write,
]);

export function isQuietResult(event: WithFacts): boolean {
  return event.tool?.quiet ?? false;
}

/** Ids of Claude calls whose row is redundant: an edit whose successful
 * result carries a diff (the diff header names the file), and a read of that
 * same file right before it. */
export function foldedCallIds(events: readonly WithFacts[]): Set<string> {
  const patched = new Set<string>();
  for (const { tool } of events)
    if (tool?.patched && tool.tool_use_id !== null)
      patched.add(tool.tool_use_id);
  const folded = new Set<string>(patched);
  let read: { id: string; path: string | null } | null = null;
  for (const { tool } of events) {
    if (tool?.shape === ToolShape.ToolResult) continue;
    if (tool?.shape !== ToolShape.ToolUse || tool.tool_use_id === null) {
      read = null;
      continue;
    }
    if (tool.kind === ToolKind.Read)
      read = { id: tool.tool_use_id, path: tool.path };
    else {
      if (
        read &&
        (tool.kind === ToolKind.Edit || tool.kind === ToolKind.Write) &&
        patched.has(tool.tool_use_id) &&
        read.path === tool.path
      )
        folded.add(read.id);
      read = null;
    }
  }
  return folded;
}

export function toolUseId(event: WithFacts): string | null {
  return event.tool?.tool_use_id ?? null;
}

/** A tool call event as the row renders it. Claude's argument is a path for
 * file tools, shortened here because only this process knows its cwd. */
export function toolCall(
  event: WithFacts & { content: string | null },
): ToolCall {
  const facts = event.tool;
  const kind = facts?.kind ?? ToolKind.Other;
  const argument = facts?.argument ?? event.content ?? "";
  const isPath = facts?.shape === ToolShape.ToolUse && PATH_KINDS.has(kind);
  return {
    kind,
    name: facts?.name ?? null,
    ...splitArgument(isPath ? shortenPath(argument) : argument),
  };
}

/** A tool result's output as the row renders it: the head of the text and
 * how many lines were left out. */
export function toolResult(
  content: string | null,
): Pick<ToolCall, "lines" | "hidden"> {
  return splitArgument(content ?? "");
}

export function toolGlyph(kind: ToolKind): string {
  switch (kind) {
    case ToolKind.Shell:
      return "$";
    case ToolKind.Read:
      return "≡";
    case ToolKind.Edit:
      return "±";
    case ToolKind.Write:
      return "+";
    case ToolKind.Search:
      return "◎";
    case ToolKind.Fetch:
      return "↗";
    case ToolKind.Agent:
      return "◈";
    case ToolKind.Todo:
      return "☐";
    case ToolKind.Other:
      return "◌";
  }
}

export function toolTone(kind: ToolKind): Tone {
  switch (kind) {
    case ToolKind.Shell:
      return Tone.Warning;
    case ToolKind.Edit:
    case ToolKind.Write:
      return Tone.Success;
    case ToolKind.Read:
    case ToolKind.Search:
    case ToolKind.Fetch:
      return Tone.Accent;
    case ToolKind.Agent:
    case ToolKind.Todo:
    case ToolKind.Other:
      return Tone.Muted;
  }
}
