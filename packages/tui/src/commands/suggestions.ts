import { matchThreads } from "@commands/lookups.ts";
import { COMMANDS } from "@commands/registries.ts";
import { KNOWN_HARNESSES } from "@harness/harnesses.ts";
import type { Thread } from "@session/threads.ts";

export type CommandSuggestion = {
  /** The bold, primary text — a command name, a harness, a thread identity. */
  label: string;
  /** Composer text to set on accept. */
  insertText: string;
  /** Dim secondary text — what the option does, not a type placeholder. */
  hint: string;
};

export type SuggestContext = {
  installedHarnesses: readonly string[];
  threads: readonly Thread[];
  pendingDeleteId: string | null;
};

/** The command name being typed, or `null` once a space starts its arguments. */
export function commandNamePrefix(text: string): string | null {
  const match = /^\/(\S*)$/.exec(text);
  return match ? (match[1] ?? "").toLowerCase() : null;
}

/** The command name plus its still-being-typed first argument, or `null`
 * once a second token starts. */
function firstArgumentPrefix(
  text: string,
): { name: string; prefix: string } | null {
  const match = /^\/(\S+)[ \t]+(\S*)$/.exec(text);
  if (!match) return null;
  return { name: (match[1] ?? "").toLowerCase(), prefix: match[2] ?? "" };
}

/** True when `text` already spells out one of `items`, so Enter should submit
 * it instead of re-accepting the same suggestion. */
export function isCompleteSuggestion(
  text: string,
  items: readonly CommandSuggestion[],
): boolean {
  const typed = text.trim();
  return items.some((item) => item.insertText.trim() === typed);
}

function nameSuggestions(prefix: string): CommandSuggestion[] {
  return Object.entries(COMMANDS)
    .filter(([name]) => name.startsWith(prefix))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, entry]) => ({
      label: `/${name}`,
      insertText: `/${name} `,
      // Summary only: arguments reveal their real values one level in.
      hint: entry.summary,
    }));
}

function harnessSuggestions(
  commandName: string,
  prefix: string,
  installed: readonly string[],
): CommandSuggestion[] {
  const lower = prefix.toLowerCase();
  return installed
    .filter((harness) => harness.startsWith(lower))
    .map((harness) => ({
      label: harness,
      insertText: `/${commandName} ${harness} `,
      hint:
        commandName === "new"
          ? `Start a new ${harness} thread`
          : `Reassign this thread to ${harness}`,
    }));
}

function threadSuggestions(
  commandName: string,
  prefix: string,
  threads: readonly Thread[],
  listAllWhenEmpty = false,
): CommandSuggestion[] {
  const matches =
    listAllWhenEmpty && prefix.trim() === ""
      ? threads
      : matchThreads(threads, prefix);
  return matches.map((thread) => ({
    label: `${thread.id.slice(0, 8)} (${thread.active_harness ?? "unassigned"})`,
    insertText: `/${commandName} ${thread.id} `,
    hint:
      thread.opening_message?.replace(/\s+/g, " ").trim().slice(0, 60) ||
      "(no message yet)",
  }));
}

function deleteSuggestions(
  prefix: string,
  ctx: SuggestContext,
): CommandSuggestion[] {
  if (!ctx.pendingDeleteId)
    return threadSuggestions("delete", prefix, ctx.threads);
  const idPrefix = ctx.pendingDeleteId.slice(0, 8);
  const candidates: CommandSuggestion[] = [
    {
      label: "confirm",
      insertText: `/delete confirm ${idPrefix} `,
      hint: "Permanently delete the pending thread",
    },
    {
      label: "cancel",
      insertText: "/delete cancel",
      hint: "Back out without deleting",
    },
  ];
  const lower = prefix.toLowerCase();
  return candidates.filter((item) => item.label.startsWith(lower));
}

const ARGUMENT_SUGGESTERS: Record<
  string,
  (prefix: string, ctx: SuggestContext) => CommandSuggestion[]
> = {
  new: (prefix, ctx) =>
    harnessSuggestions("new", prefix, ctx.installedHarnesses),
  harness: (prefix, ctx) =>
    harnessSuggestions("harness", prefix, ctx.installedHarnesses),
  resume: (prefix, ctx) =>
    threadSuggestions("resume", prefix, ctx.threads, true),
  archive: (prefix, ctx) => threadSuggestions("archive", prefix, ctx.threads),
  delete: deleteSuggestions,
  mode: (prefix) =>
    [
      {
        label: "plan",
        insertText: "/mode plan",
        hint: "Read-only: the harness plans and proposes",
      },
      {
        label: "build",
        insertText: "/mode build",
        hint: "The harness may edit and run commands",
      },
    ].filter((item) => item.label.startsWith(prefix.toLowerCase())),
};

/** `/hooks` takes a subcommand, then any number of harnesses after `init`;
 * `null` when `text` is not a `/hooks` invocation. */
function hookSuggestions(text: string): CommandSuggestion[] | null {
  const match = /^\/hooks[ \t]+(.*)$/is.exec(text);
  if (!match) return null;
  const rest = match[1] ?? "";
  const tokens = rest.split(/\s+/);
  const prefix = (tokens.pop() ?? "").toLowerCase();

  if (tokens.length === 0) {
    return [
      {
        label: "status",
        insertText: "/hooks status",
        hint: "Show whether each harness's hook is installed and approved",
      },
      {
        label: "init",
        insertText: "/hooks init ",
        hint: "Install the hooks, backing each config file up first",
      },
    ].filter((item) => item.label.startsWith(prefix));
  }
  if (tokens[0]?.toLowerCase() !== "init") return [];

  const chosen = tokens.slice(1);
  return KNOWN_HARNESSES.filter(
    (harness) => harness.startsWith(prefix) && !chosen.includes(harness),
  ).map((harness) => ({
    label: harness,
    insertText: `/hooks init ${[...chosen, harness].join(" ")} `,
    hint: `Install the ${harness} hook`,
  }));
}

/** The complete, sorted suggestion set for the composer text. Uncapped;
 * the UI bounds how many render. */
export function suggestFor(
  text: string,
  ctx: SuggestContext,
): CommandSuggestion[] {
  const namePrefix = commandNamePrefix(text);
  if (namePrefix !== null) return nameSuggestions(namePrefix);

  const hooks = hookSuggestions(text);
  if (hooks) return hooks;

  const arg = firstArgumentPrefix(text);
  if (!arg) return [];
  const provider = ARGUMENT_SUGGESTERS[arg.name];
  return provider ? provider(arg.prefix, ctx) : [];
}
