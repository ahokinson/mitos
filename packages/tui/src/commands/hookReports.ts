import {
  type HookInitOutcome,
  HookInitResult,
  type HookStatus,
  HookTrust,
} from "@session/hooks.ts";

const SECOND_MS = 1000;
const MINUTE_S = 60;
const HOUR_S = 3600;
const DAY_S = 86_400;

/** What the user must do for the harness to start using an installed hook. */
const APPROVAL_STEP: Record<string, string> = {
  codex: "approve it with /hooks inside Codex",
  hermes: "approve it on Hermes's first run, or set hooks_auto_accept: true",
};

export function relativeTime(iso: string, now: Date = new Date()): string {
  const seconds = Math.round((now.getTime() - Date.parse(iso)) / SECOND_MS);
  if (!Number.isFinite(seconds)) return "at an unknown time";
  if (seconds < MINUTE_S) return "just now";
  if (seconds < HOUR_S) return `${Math.floor(seconds / MINUTE_S)}m ago`;
  if (seconds < DAY_S) return `${Math.floor(seconds / HOUR_S)}h ago`;
  return `${Math.floor(seconds / DAY_S)}d ago`;
}

function stateOf(status: HookStatus): string {
  if (!status.installed) return "not installed";
  if (status.trust !== HookTrust.Untrusted) return "installed";
  const step = APPROVAL_STEP[status.harness];
  return step
    ? `installed, NOT APPROVED yet: ${step}`
    : "installed, NOT APPROVED yet by the harness";
}

/** One block per harness: its state, then what is wrong or notable. */
export function statusLines(
  statuses: readonly HookStatus[],
  now: Date = new Date(),
): string[] {
  const lines: string[] = [];
  for (const status of statuses) {
    lines.push(`${status.harness.padEnd(9)} ${stateOf(status)}`);
    lines.push(`          ${status.target}`);
    if (status.blocked_by)
      lines.push(`          cannot write: ${status.blocked_by}`);
    for (const problem of status.problems)
      lines.push(`          problem: ${problem}`);
    lines.push(
      status.last_seen
        ? `          last data ${relativeTime(status.last_seen, now)}`
        : "          no data received yet",
    );
  }
  lines.push("Run /hooks init to install; it backs each file up first.");
  return lines;
}

const RESULT_LABEL: Record<HookInitResult, string> = {
  [HookInitResult.Installed]: "installed",
  [HookInitResult.AlreadyInstalled]: "already installed",
  [HookInitResult.Skipped]: "skipped",
  [HookInitResult.Manual]: "manual step",
};

export function initLines(outcomes: readonly HookInitOutcome[]): string[] {
  const lines: string[] = [];
  for (const outcome of outcomes) {
    lines.push(
      `${outcome.harness.padEnd(9)} ${RESULT_LABEL[outcome.result]}: ${outcome.message}`,
    );
    if (outcome.backup) lines.push(`          backup: ${outcome.backup}`);
    for (const line of outcome.snippet?.split("\n") ?? [])
      lines.push(`          | ${line}`);
  }
  if (outcomes.some((o) => o.result === HookInitResult.Installed))
    lines.push("Run /hooks to check that each harness has approved its hook.");
  return lines;
}
