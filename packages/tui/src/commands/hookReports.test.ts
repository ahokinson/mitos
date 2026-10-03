import { expect, test } from "bun:test";

import { initLines, relativeTime, statusLines } from "@commands/hookReports.ts";
import {
  type HookInitOutcome,
  HookInitResult,
  type HookStatus,
  HookTrust,
} from "@session/hooks.ts";

const NOW = new Date("2026-10-03T12:00:00Z");

function status(
  overrides: Partial<HookStatus> & { harness: string },
): HookStatus {
  return {
    target: `/home/me/.${overrides.harness}/config`,
    installed: true,
    trust: HookTrust.NotApplicable,
    blocked_by: null,
    problems: [],
    last_seen: null,
    ...overrides,
  };
}

test("relative times step from seconds to days", () => {
  const ago = (seconds: number) =>
    relativeTime(new Date(NOW.getTime() - seconds * 1000).toISOString(), NOW);
  expect(ago(5)).toBe("just now");
  expect(ago(150)).toBe("2m ago");
  expect(ago(7200)).toBe("2h ago");
  expect(ago(3 * 86_400)).toBe("3d ago");
  expect(relativeTime("not a date", NOW)).toBe("at an unknown time");
});

test("an installed, reporting hook reads as plainly installed", () => {
  const lines = statusLines(
    [status({ harness: "claude", last_seen: "2026-10-03T11:58:00Z" })],
    NOW,
  );
  expect(lines[0]).toBe("claude    installed");
  expect(lines).toContain("          last data 2m ago");
});

test("an untrusted codex hook says it is not approved and how to approve it", () => {
  const lines = statusLines(
    [status({ harness: "codex", trust: HookTrust.Untrusted })],
    NOW,
  );
  expect(lines[0]).toBe(
    "codex     installed, NOT APPROVED yet: approve it with /hooks inside Codex",
  );
  expect(lines).toContain("          no data received yet");
});

test("an untrusted hermes hook names both ways to approve it", () => {
  const lines = statusLines(
    [status({ harness: "hermes", trust: HookTrust.Untrusted })],
    NOW,
  );
  expect(lines[0]).toContain("NOT APPROVED yet");
  expect(lines[0]).toContain("hooks_auto_accept: true");
});

test("an untrusted hook of an unknown harness still says it is not approved", () => {
  const lines = statusLines(
    [status({ harness: "future", trust: HookTrust.Untrusted })],
    NOW,
  );
  expect(lines[0]).toBe("future    installed, NOT APPROVED yet by the harness");
});

test("a read-only target shows why Mitos cannot write it", () => {
  const lines = statusLines(
    [
      status({
        harness: "hermes",
        installed: false,
        blocked_by: "managed by Nix, read-only (/nix/store/abc-config.yaml)",
      }),
    ],
    NOW,
  );
  expect(lines[0]).toBe("hermes    not installed");
  expect(lines).toContain(
    "          cannot write: managed by Nix, read-only (/nix/store/abc-config.yaml)",
  );
});

test("problems with the target are listed", () => {
  const lines = statusLines(
    [
      status({
        harness: "claude",
        problems: ["settings.json is not valid JSON"],
      }),
    ],
    NOW,
  );
  expect(lines).toContain("          problem: settings.json is not valid JSON");
});

test("status always ends by pointing at /hooks init", () => {
  expect(statusLines([], NOW).at(-1)).toContain("/hooks init");
});

function outcome(overrides: Partial<HookInitOutcome>): HookInitOutcome {
  return {
    harness: "claude",
    result: HookInitResult.Installed,
    message: "done",
    backup: null,
    snippet: null,
    ...overrides,
  };
}

test("init lines show the result, the backup, and then remind about approval", () => {
  const lines = initLines([
    outcome({ backup: "/h/settings.json.mitos-backup-1" }),
  ]);
  expect(lines[0]).toBe("claude    installed: done");
  expect(lines[1]).toBe("          backup: /h/settings.json.mitos-backup-1");
  expect(lines.at(-1)).toContain("check that each harness has approved");
});

test("a manual step prints its snippet indented and no approval reminder", () => {
  const lines = initLines([
    outcome({
      harness: "hermes",
      result: HookInitResult.Manual,
      message: "add these entries",
      snippet: "hooks:\n  on_session_end:",
    }),
  ]);
  expect(lines).toEqual([
    "hermes    manual step: add these entries",
    "          | hooks:",
    "          |   on_session_end:",
  ]);
});

test("skipped and already-installed results are labelled", () => {
  const lines = initLines([
    outcome({ harness: "codex", result: HookInitResult.AlreadyInstalled }),
    outcome({
      harness: "hermes",
      result: HookInitResult.Skipped,
      message: "not changed: read-only",
    }),
  ]);
  expect(lines[0]).toBe("codex     already installed: done");
  expect(lines[1]).toBe("hermes    skipped: not changed: read-only");
});
