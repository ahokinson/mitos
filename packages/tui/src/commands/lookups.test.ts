import { expect, test } from "bun:test";

import { formatThreadLine, matchThreads } from "@commands/lookups.ts";
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

const CLAUDE = thread({
  id: "11111111-aaaa-bbbb-cccc-111111111111",
  active_harness: "claude",
  opening_message: "fix the login bug",
});
const CODEX_A = thread({
  id: "22222222-aaaa-bbbb-cccc-222222222222",
  active_harness: "codex",
  opening_message: "refactor the parser",
});
const CODEX_B = thread({
  id: "33333333-aaaa-bbbb-cccc-333333333333",
  active_harness: "codex",
  opening_message: "write release notes",
});
const THREADS = [CLAUDE, CODEX_A, CODEX_B];

test("empty query matches nothing", () => {
  expect(matchThreads(THREADS, "")).toEqual([]);
  expect(matchThreads(THREADS, "   ")).toEqual([]);
});

test("an exact id match wins outright, case-insensitively", () => {
  expect(matchThreads(THREADS, CLAUDE.id.toUpperCase())).toEqual([CLAUDE]);
});

test("an id prefix matches", () => {
  expect(matchThreads(THREADS, "22222222")).toEqual([CODEX_A]);
});

test("a harness substring can match multiple threads", () => {
  expect(matchThreads(THREADS, "codex")).toEqual([CODEX_A, CODEX_B]);
});

test("an opening_message substring matches, case-insensitively", () => {
  expect(matchThreads(THREADS, "LOGIN")).toEqual([CLAUDE]);
});

test("no matches returns an empty array", () => {
  expect(matchThreads(THREADS, "nonexistent")).toEqual([]);
});

test("formatThreadLine marks the current thread", () => {
  expect(formatThreadLine(CLAUDE, CLAUDE.id)).toStartWith("*");
  expect(formatThreadLine(CODEX_A, CLAUDE.id)).toStartWith(" ");
});

test("formatThreadLine falls back when there's no opening message yet", () => {
  const draft = thread({
    id: "44444444-aaaa-bbbb-cccc-444444444444",
    opening_message: null,
  });
  expect(formatThreadLine(draft, null)).toContain("(no message yet)");
});
