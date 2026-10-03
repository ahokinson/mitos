import type { CommandContext } from "@commands/contexts.ts";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import type { Thread } from "@session/threads.ts";

/** Shared target-resolution for every command that selects a thread by typed
 * text (`/resume`, `/archive`, `/delete`). An exact id match always wins
 * outright; otherwise collects every thread whose id starts with the query
 * or whose harness/opening message contains it (same substring semantics
 * the old `Picker`'s `matchesQuery` used) and leaves disambiguation to the
 * caller — multiple matches means "don't act," not "pick the first one." */
export function matchThreads(
  threads: readonly Thread[],
  rawQuery: string,
): Thread[] {
  const query = rawQuery.trim().toLowerCase();
  if (query === "") return [];

  const exact = threads.find((thread) => thread.id.toLowerCase() === query);
  if (exact) return [exact];

  return threads.filter(
    (thread) =>
      thread.id.toLowerCase().startsWith(query) ||
      (thread.active_harness?.toLowerCase().includes(query) ?? false) ||
      (thread.opening_message?.toLowerCase().includes(query) ?? false),
  );
}

/** One line per thread for `/threads` and disambiguation listings. */
export function formatThreadLine(
  thread: Thread,
  currentId: string | null,
): string {
  const marker = thread.id === currentId ? "*" : " ";
  const idPrefix = thread.id.slice(0, 8);
  const harness = thread.active_harness ?? "unassigned";
  const opening =
    thread.opening_message?.replace(/\s+/g, " ").trim().slice(0, 60) ||
    "(no message yet)";
  return `${marker} ${idPrefix}  ${harness.padEnd(9)} ${thread.updated_at}  ${opening}`;
}

function candidateLines(
  matches: readonly Thread[],
  query: string,
  currentId: string | null,
): string[] {
  return [
    `Multiple threads match "${query}" — refine the query or paste a longer id:`,
    ...matches.map((thread) => formatThreadLine(thread, currentId)),
  ];
}

/** The single thread matching `query`, or `null` after noting why not. */
export function pickThread(
  ctx: CommandContext,
  query: string,
  currentId: string | null,
): Thread | null {
  const matches = matchThreads(ctx.list.threads(), query);
  const [only] = matches;
  if (!only) {
    ctx.note(
      currentId,
      FeedbackTone.Error,
      `No threads match "${query}". Run /threads to see all threads.`,
    );
    return null;
  }
  if (matches.length > 1) {
    ctx.note(
      currentId,
      FeedbackTone.Info,
      candidateLines(matches, query, currentId),
    );
    return null;
  }
  return only;
}
