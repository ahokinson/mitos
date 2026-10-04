import type { ThreadEvent } from "@session/events.ts";
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

export type ToolCall = {
  kind: ToolKind;
  /** The tool's own name, when the harness reports one separately. */
  name: string | null;
  /** Lines of the main argument, capped at `MAX_ARGUMENT_LINES`. */
  lines: string[];
  /** Lines of the argument left out of `lines`. */
  hidden: number;
};

const MAX_ARGUMENT_LINES = 4;
const MAX_LINE_LENGTH = 400;

const KINDS_BY_NAME: Record<string, ToolKind> = {
  bash: ToolKind.Shell,
  execute: ToolKind.Shell,
  commandexecution: ToolKind.Shell,
  read: ToolKind.Read,
  notebookread: ToolKind.Read,
  edit: ToolKind.Edit,
  multiedit: ToolKind.Edit,
  notebookedit: ToolKind.Edit,
  filechange: ToolKind.Edit,
  write: ToolKind.Write,
  grep: ToolKind.Search,
  glob: ToolKind.Search,
  search: ToolKind.Search,
  webfetch: ToolKind.Fetch,
  websearch: ToolKind.Fetch,
  fetch: ToolKind.Fetch,
  task: ToolKind.Agent,
  agent: ToolKind.Agent,
  todowrite: ToolKind.Todo,
};

const ARGUMENT_KEYS = [
  "command",
  "file_path",
  "notebook_path",
  "pattern",
  "path",
  "url",
  "query",
  "description",
  "prompt",
];

function record(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

function kindOf(name: unknown): ToolKind {
  return typeof name === "string"
    ? (KINDS_BY_NAME[name.toLowerCase()] ?? ToolKind.Other)
    : ToolKind.Other;
}

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

/** A successful Claude read or file edit: the call row already says what
 * happened, and the result is only file text or a confirmation sentence. */
export function isQuietResult(event: ThreadEvent): boolean {
  const payload = record(event.payload);
  if (payload.type !== "tool_result" || payload.is_error === true) return false;
  const result = record(payload.tool_use_result);
  return "file" in result || "structuredPatch" in result;
}

/** Ids of Claude calls whose row is redundant: an edit whose successful
 * result carries a diff (the diff header names the file), and a read of that
 * same file right before it. */
export function foldedCallIds(events: readonly ThreadEvent[]): Set<string> {
  const patched = new Set<string>();
  for (const event of events) {
    const payload = record(event.payload);
    if (
      payload.type === "tool_result" &&
      payload.is_error !== true &&
      typeof payload.tool_use_id === "string" &&
      "structuredPatch" in record(payload.tool_use_result)
    )
      patched.add(payload.tool_use_id);
  }
  const folded = new Set<string>(patched);
  let read: { id: string; path: unknown } | null = null;
  for (const event of events) {
    const payload = record(event.payload);
    if (payload.type === "tool_result") continue;
    if (payload.type !== "tool_use" || typeof payload.id !== "string") {
      read = null;
      continue;
    }
    const kind = kindOf(payload.name);
    const path = record(payload.input).file_path;
    if (kind === ToolKind.Read) read = { id: payload.id, path };
    else {
      if (
        read &&
        (kind === ToolKind.Edit || kind === ToolKind.Write) &&
        patched.has(payload.id) &&
        read.path === path
      )
        folded.add(read.id);
      read = null;
    }
  }
  return folded;
}

/** The id that ties a Claude call to its result. */
export function toolUseId(event: ThreadEvent): string | null {
  const payload = record(event.payload);
  const id = payload.type === "tool_use" ? payload.id : payload.tool_use_id;
  return typeof id === "string" ? id : null;
}

/** A tool call event as the row renders it. Claude reports only the tool name
 * as `content`, so its argument comes from the `tool_use` input; the other
 * harnesses already describe the call in `content`. */
export function toolCall(event: ThreadEvent): ToolCall {
  const payload = record(event.payload);
  const content = event.content ?? "";
  if (payload.type === "tool_use") {
    const input = record(payload.input);
    const key = ARGUMENT_KEYS.find(
      (candidate) =>
        typeof input[candidate] === "string" && input[candidate] !== "",
    );
    const kind = kindOf(payload.name);
    const argument = key === undefined ? "" : (input[key] as string);
    return {
      kind,
      name: content,
      ...splitArgument(PATH_KINDS.has(kind) ? shortenPath(argument) : argument),
    };
  }
  const kind = [payload.tool, payload.kind, payload.type]
    .map(kindOf)
    .find((candidate) => candidate !== ToolKind.Other);
  return {
    kind: kind ?? ToolKind.Other,
    name: null,
    ...splitArgument(content),
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
