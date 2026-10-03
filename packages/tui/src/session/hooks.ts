export enum HookTrust {
  NotApplicable = "not_applicable",
  Trusted = "trusted",
  /** Installed, but the harness skips it until the user approves it. */
  Untrusted = "untrusted",
}

export enum HookInitResult {
  Installed = "installed",
  AlreadyInstalled = "already_installed",
  /** Nothing was changed; the message says why. */
  Skipped = "skipped",
  /** Mitos will not edit this one; the snippet is for the user to add. */
  Manual = "manual",
}

/** One harness's hook as `mitos hooks status --json` reports it. */
export type HookStatus = {
  harness: string;
  target: string;
  installed: boolean;
  trust: HookTrust;
  /** Why Mitos cannot write the target; null when it can. */
  blocked_by: string | null;
  problems: string[];
  last_seen: string | null;
};

export type HookInitOutcome = {
  harness: string;
  result: HookInitResult;
  message: string;
  backup: string | null;
  snippet: string | null;
};
