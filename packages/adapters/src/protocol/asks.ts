import type { ReplyAction } from "@protocol/actions.ts";

export enum RequestKind {
  Question = "question",
  Permission = "permission",
  PlanApproval = "plan_approval",
}

/** `payload` of a `request` event. `id` is the adapter's own id, echoed back
 * as `request_id` in the matching `answer` line. */
export type RequestPayload = {
  id: string;
  kind: RequestKind;
  [key: string]: unknown;
};

/** One stdin line written by core in reply to a `request` event. */
export type AnswerMessage = {
  action: ReplyAction.Answer;
  request_id: string;
  response: unknown;
};
