import type { HandoffUsage } from "@protocol/responses.ts";

export enum AdapterEventKind {
  AssistantDelta = "assistant_delta",
  AssistantMessage = "assistant_message",
  ToolCall = "tool_call",
  ToolResult = "tool_result",
  Status = "status",
  Usage = "usage",
  Error = "error",
  NativeSessionUpdate = "native_session_update",
  TurnComplete = "turn_complete",
  Request = "request",
}

/** One line of an adapter's NDJSON event stream during `start_thread`,
 * `attach_thread`, or `send_message`. */
export type AdapterEvent = {
  event: AdapterEventKind;
  turn_id?: string;
  role?: string;
  content?: string;
  payload?: unknown;
  usage?: HandoffUsage;
  native_session?: unknown | null;
  timestamp?: string;
};
