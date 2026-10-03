import type { Harness } from "@protocol/harnesses.ts";

export const PROTOCOL_VERSION = 1;

export enum ThreadMode {
  Plan = "plan",
  Build = "build",
}

export enum Action {
  PrepareLaunch = "prepare_launch",
  CollectHandoff = "collect_handoff",
  Negotiate = "negotiate",
  StartThread = "start_thread",
  AttachThread = "attach_thread",
  SendMessage = "send_message",
  DetachThread = "detach_thread",
}

export type PrepareLaunchRequest = {
  protocol_version: number;
  action: Action.PrepareLaunch;
  harness: Harness;
  workdir: string;
  context: string;
  native_session: unknown | null;
};

export type CollectHandoffRequest = {
  protocol_version: number;
  action: Action.CollectHandoff;
  harness: Harness;
  workdir: string;
  launched_at: string;
  native_session: unknown | null;
};

export type NegotiateRequest = {
  protocol_version: number;
  action: Action.Negotiate;
  harness: Harness;
};

export type StartThreadRequest = {
  protocol_version: number;
  action: Action.StartThread;
  harness: Harness;
  thread_id: string;
  workdir: string;
  mode: ThreadMode;
  initial_context: string;
};

export type AttachThreadRequest = {
  protocol_version: number;
  action: Action.AttachThread;
  harness: Harness;
  thread_id: string;
  workdir: string;
  native_session: unknown | null;
  since_cursor: number;
};

export type SendMessageRequest = {
  protocol_version: number;
  action: Action.SendMessage;
  harness: Harness;
  thread_id: string;
  workdir: string;
  native_session: unknown | null;
  mode: ThreadMode;
  turn_id: string;
  text: string;
};

export type DetachThreadRequest = {
  protocol_version: number;
  action: Action.DetachThread;
  harness: Harness;
  native_session: unknown | null;
};

export type AdapterRequest =
  | PrepareLaunchRequest
  | CollectHandoffRequest
  | NegotiateRequest
  | StartThreadRequest
  | AttachThreadRequest
  | SendMessageRequest
  | DetachThreadRequest;

export enum ReplyAction {
  Answer = "answer",
}

export const INITIAL_ACTIONS: readonly string[] = Object.values(Action);
