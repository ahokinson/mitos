import { expect, test } from "bun:test";

import {
  type CommandSuggestion,
  commandNamePrefix,
  isCompleteSuggestion,
  type SuggestContext,
  suggestFor,
} from "@commands/suggestions.ts";
import { type Thread, ThreadMode, ThreadStatus } from "@session/threads.ts";

function thread(overrides: Partial<Thread> & { id: string }): Thread {
  return {
    workspace_id: "ws1",
    status: ThreadStatus.Active,
    mode: ThreadMode.Build,
    active_harness: null,
    native_session: null,
    last_event_seq: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    opening_message: null,
    ...overrides,
  };
}

function labels(items: readonly CommandSuggestion[]): string[] {
  return items.map((item) => item.label);
}

const CLAUDE_THREAD = thread({
  id: "11111111-aaaa-bbbb-cccc-111111111111",
  active_harness: "claude",
  opening_message: "fix the bug",
});
const CODEX_THREAD = thread({
  id: "22222222-aaaa-bbbb-cccc-222222222222",
  active_harness: "codex",
  opening_message: "write docs",
});

function ctx(overrides: Partial<SuggestContext> = {}): SuggestContext {
  return {
    installedHarnesses: ["claude", "codex"],
    threads: [CLAUDE_THREAD, CODEX_THREAD],
    pendingDeleteId: null,
    ...overrides,
  };
}

test("a bare slash yields an empty command-name prefix", () => {
  expect(commandNamePrefix("/")).toBe("");
});

test("a space after the command name ends prefix mode", () => {
  expect(commandNamePrefix("/resume so")).toBeNull();
});

test("plain chat text yields no prefix", () => {
  expect(commandNamePrefix("hello")).toBeNull();
});

test("level 1: a bare slash suggests every command, sorted, uncapped, with no raw placeholder in the hint", () => {
  const items = suggestFor("/", ctx());
  expect(labels(items)).toEqual([...labels(items)].sort());
  expect(labels(items)).toContain("/resume");
  for (const item of items) {
    expect(item.hint).not.toMatch(/[<[]/);
  }
});

test("level 1: a narrowing prefix filters down to a single command", () => {
  expect(labels(suggestFor("/res", ctx()))).toEqual(["/resume"]);
});

test("level 1: a partial name filters down", () => {
  expect(labels(suggestFor("/de", ctx()))).toEqual([
    "/decision",
    "/delete",
    "/deny",
  ]);
});

test("level 2: /new with nothing typed yet offers every installed harness", () => {
  expect(labels(suggestFor("/new ", ctx()))).toEqual(["claude", "codex"]);
});

test("level 2: /new filters harnesses by prefix", () => {
  expect(labels(suggestFor("/new cod", ctx()))).toEqual(["codex"]);
});

test("level 2 ends once the argument is complete and a trailing space follows", () => {
  expect(suggestFor("/new codex ", ctx())).toEqual([]);
});

test("level 2: /harness offers installed harnesses too", () => {
  expect(labels(suggestFor("/harness ", ctx()))).toEqual(["claude", "codex"]);
});

test("a command with no argument suggester yields nothing at level 2", () => {
  expect(suggestFor("/threads ", ctx())).toEqual([]);
  expect(suggestFor("/note ", ctx())).toEqual([]);
});

test("level 2: /resume suggests matching threads, not a <query> placeholder", () => {
  const items = suggestFor("/resume fix", ctx());
  expect(items).toHaveLength(1);
  expect(items[0]?.insertText).toBe(`/resume ${CLAUDE_THREAD.id} `);
  expect(items[0]?.label).toContain("claude");
});

test("level 2: /archive suggests matching threads by harness", () => {
  const items = suggestFor("/archive codex", ctx());
  expect(items).toHaveLength(1);
  expect(items[0]?.insertText).toBe(`/archive ${CODEX_THREAD.id} `);
});

test("level 2: /delete with no pending delete suggests matching threads", () => {
  const items = suggestFor("/delete fix", ctx());
  expect(items[0]?.insertText).toBe(`/delete ${CLAUDE_THREAD.id} `);
});

test("level 2: /delete with a pending delete suggests confirm/cancel instead of threads", () => {
  const items = suggestFor(
    "/delete ",
    ctx({ pendingDeleteId: CLAUDE_THREAD.id }),
  );
  expect(labels(items)).toEqual(["confirm", "cancel"]);
  expect(items[0]?.insertText).toBe(
    `/delete confirm ${CLAUDE_THREAD.id.slice(0, 8)} `,
  );
});

test("level 2: typing narrows confirm/cancel by prefix", () => {
  const items = suggestFor(
    "/delete ca",
    ctx({ pendingDeleteId: CLAUDE_THREAD.id }),
  );
  expect(labels(items)).toEqual(["cancel"]);
});

test("/mode suggests plan and build", () => {
  expect(labels(suggestFor("/mode ", ctx()))).toEqual(["plan", "build"]);
  expect(labels(suggestFor("/mode pl", ctx()))).toEqual(["plan"]);
});

test("a fully typed argument counts as complete so Enter can submit it", () => {
  const items = suggestFor("/mode plan", ctx());
  expect(labels(items)).toEqual(["plan"]);
  expect(isCompleteSuggestion("/mode plan", items)).toBe(true);
  expect(isCompleteSuggestion("/mode pl", items)).toBe(false);
});

test("a fully typed command name is complete; a partial one is not", () => {
  const items = suggestFor("/help", ctx());
  expect(isCompleteSuggestion("/help", items)).toBe(true);
  expect(isCompleteSuggestion("/hel", items)).toBe(false);
});

test("/resume lists every thread as soon as the space is typed", () => {
  const items = suggestFor("/resume ", ctx());
  expect(items.map((item) => item.insertText)).toEqual([
    `/resume ${CLAUDE_THREAD.id} `,
    `/resume ${CODEX_THREAD.id} `,
  ]);
});

test("/hooks suggests its subcommands", () => {
  expect(labels(suggestFor("/hooks ", ctx()))).toEqual(["status", "init"]);
  expect(labels(suggestFor("/hooks in", ctx()))).toEqual(["init"]);
  expect(suggestFor("/hooks x", ctx())).toEqual([]);
});

test("/hooks init suggests every known harness, installed or not", () => {
  const items = suggestFor("/hooks init ", ctx({ installedHarnesses: [] }));
  expect(labels(items)).toEqual(["claude", "codex", "opencode", "hermes"]);
  expect(labels(suggestFor("/hooks init co", ctx()))).toEqual(["codex"]);
});

test("/hooks init skips harnesses already typed and keeps them on accept", () => {
  const items = suggestFor("/hooks init codex ", ctx());
  expect(labels(items)).toEqual(["claude", "opencode", "hermes"]);
  expect(items[0]?.insertText).toBe("/hooks init codex claude ");
});

test("/hooks status takes no harness arguments", () => {
  expect(suggestFor("/hooks status ", ctx())).toEqual([]);
});

test("/archive and /delete stay quiet on an empty argument", () => {
  expect(suggestFor("/archive ", ctx())).toEqual([]);
  expect(suggestFor("/delete ", ctx())).toEqual([]);
});
