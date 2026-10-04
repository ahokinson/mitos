export enum RequestKind {
  Question = "question",
  Permission = "permission",
  PlanApproval = "plan_approval",
}

export enum RequestStatus {
  Pending = "pending",
  Answered = "answered",
  Cancelled = "cancelled",
}

export enum Decision {
  Approve = "approve",
  Deny = "deny",
}

/** Mirrors `crates/mitos/src/domain/requests.rs::HarnessRequest`. `payload` is the
 * adapter's own request payload (`id`, `kind`, `title`, ...). */
export type HarnessRequest = {
  id: string;
  thread_id: string;
  turn_id: string | null;
  harness: string | null;
  kind: RequestKind;
  payload: unknown | null;
  status: RequestStatus;
  response: unknown | null;
  created_at: string;
  answered_at: string | null;
};

const KIND_LABELS: Record<RequestKind, string> = {
  [RequestKind.Question]: "Question",
  [RequestKind.Permission]: "Permission",
  [RequestKind.PlanApproval]: "Plan ready",
};

function titleOf(payload: unknown): string | null {
  if (typeof payload !== "object" || payload === null) return null;
  const title = (payload as { title?: unknown }).title;
  return typeof title === "string" && title.trim() ? title.trim() : null;
}

export function describeRequest(kind: RequestKind, payload: unknown): string {
  return `${KIND_LABELS[kind]}: ${titleOf(payload) ?? "(no details)"}`;
}

export function requestHint(kind: RequestKind): string {
  switch (kind) {
    case RequestKind.Question:
      return "/answer <text>";
    case RequestKind.Permission:
      return "/approve · /deny";
    case RequestKind.PlanApproval:
      return "/approve to start building · /deny <feedback>";
  }
}

/** What the user chose. The core turns it into the response each adapter
 * expects: a decision for a permission or plan, text for a question or a
 * denial's reason. */
export type Reply = { decision?: Decision; text?: string };
