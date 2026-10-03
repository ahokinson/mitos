import type { ThreadMode } from "@protocol/actions.ts";

export enum ResponseKind {
  Launch = "launch",
  Capabilities = "capabilities",
  Handoff = "handoff",
}

export type LaunchResponse = {
  protocol_version: number;
  kind: ResponseKind.Launch;
  program: string;
  args: string[];
  env?: Record<string, string>;
  native_session?: unknown | null;
};

export type Capabilities = {
  headless: boolean;
  streaming?: boolean;
  detach?: boolean;
  /** Modes the adapter can enforce; omitted means build-only. */
  modes?: ThreadMode[];
  /** Can raise `request` events and accept `answer` lines on stdin. */
  ask_back?: boolean;
};

export type NegotiateResponse = {
  protocol_version: number;
  kind: ResponseKind.Capabilities;
  capabilities: Capabilities;
};

export type HandoffUsage = {
  input_tokens?: number;
  output_tokens?: number;
  cached_input_tokens?: number;
  /** Cost in USD as the harness reports it; absent when it reports none. */
  cost_usd?: number;
  /** Current token occupancy of the native session's context window. */
  context_used_tokens?: number;
  /** Maximum token capacity of that native session's context window. */
  context_limit_tokens?: number;
  model?: string;
  turns?: number;
  /** Percentage (0-100) of the account's rolling 5-hour plan window consumed. */
  plan_five_hour_percent?: number;
  plan_five_hour_resets_at?: string;
  plan_week_percent?: number;
  plan_week_resets_at?: string;
};

export type HandoffResponse = {
  protocol_version: number;
  kind: ResponseKind.Handoff;
  native_session?: unknown | null;
  transcript?: {
    source: string;
    messages: Array<{ role: string; text: string }>;
    truncated: boolean;
  } | null;
  usage?: HandoffUsage | null;
};
