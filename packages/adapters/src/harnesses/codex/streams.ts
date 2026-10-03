import { asRecord, isRecord, type JsonRecord } from "@json/records.ts";
import { clip } from "@json/texts.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { RequestKind, type RequestPayload } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";
import type { HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";
import type { RpcId } from "@rpc/peers.ts";

/** Per-turn approval and sandbox settings. Codex never asks for approval;
 * plan is read-only, build is unrestricted (cerberus guards the harness). */
export function turnPolicy(mode: ThreadMode): {
  approvalPolicy: string;
  sandboxPolicy: JsonRecord;
} {
  return {
    approvalPolicy: "never",
    sandboxPolicy: {
      type: mode === ThreadMode.Plan ? "readOnly" : "dangerFullAccess",
    },
  };
}

/** The thread-level `sandbox` value matching `turnPolicy`. */
export function threadSandbox(mode: ThreadMode): string {
  return mode === ThreadMode.Plan ? "read-only" : "danger-full-access";
}

type TokenCounts = { input?: number; output?: number; cached?: number };

/** Codex reports thread-lifetime token totals; Mitos wants what one turn
 * consumed. Whatever the thread had used before the turn is the baseline:
 * the last total seen before `begin()` (a resumed thread replays it), else
 * the first in-turn total minus its `last` request. */
export class TurnTokens {
  private began = false;
  private baseline: Required<TokenCounts> | null = null;

  begin(): void {
    this.began = true;
  }

  /** Token figures for this turn alone, or `undefined` when the baseline
   * can't be known. Updates before `begin()` only set the baseline. */
  consumed(tokenUsage: unknown): TokenCounts | undefined {
    const usage = asRecord(tokenUsage);
    const total = countsOf(usage?.total);
    if (!total) return undefined;
    if (!this.began) {
      this.baseline = total;
      return undefined;
    }
    if (!this.baseline) {
      const last = countsOf(usage?.last);
      if (!last) return undefined;
      this.baseline = {
        input: Math.max(total.input - last.input, 0),
        output: Math.max(total.output - last.output, 0),
        cached: Math.max(total.cached - last.cached, 0),
      };
    }
    return {
      input: Math.max(total.input - this.baseline.input, 0),
      output: Math.max(total.output - this.baseline.output, 0),
      cached: Math.max(total.cached - this.baseline.cached, 0),
    };
  }
}

function countsOf(value: unknown): Required<TokenCounts> | null {
  const breakdown = asRecord(value);
  const input = numberValue(breakdown?.inputTokens);
  const output = numberValue(breakdown?.outputTokens);
  const cached = numberValue(breakdown?.cachedInputTokens);
  if (input === undefined || output === undefined || cached === undefined)
    return null;
  return { input, output, cached };
}

/** Maps `codex app-server` notifications onto Mitos's streaming adapter
 * events. Reasoning items are intentionally not surfaced. `turn/completed`
 * is not handled here: the caller owns the end of the turn. */
export function processNotification(
  method: string,
  params: unknown,
  turnId: string | undefined,
  tokens: TurnTokens = new TurnTokens(),
): AdapterEvent[] {
  const body = asRecord(params);
  if (!body) return [];
  switch (method) {
    case "item/agentMessage/delta": {
      const delta = stringValue(body.delta);
      return delta
        ? [
            {
              event: AdapterEventKind.AssistantDelta,
              turn_id: turnId,
              role: MessageRole.Assistant,
              content: delta,
            },
          ]
        : [];
    }
    case "item/started": {
      const item = asRecord(body.item);
      if (item?.type === "commandExecution") {
        const command = stringValue(item.command);
        return command
          ? [
              {
                event: AdapterEventKind.ToolCall,
                turn_id: turnId,
                content: command,
                payload: item,
              },
            ]
          : [];
      }
      if (item?.type === "fileChange") {
        return [
          {
            event: AdapterEventKind.ToolCall,
            turn_id: turnId,
            content: describeChanges(item),
            payload: item,
          },
        ];
      }
      return [];
    }
    case "item/completed": {
      const item = asRecord(body.item);
      if (!item) return [];
      if (item.type === "agentMessage") {
        const text = stringValue(item.text);
        return text
          ? [
              {
                event: AdapterEventKind.AssistantMessage,
                turn_id: turnId,
                role: MessageRole.Assistant,
                content: text,
              },
            ]
          : [];
      }
      if (item.type === "plan") {
        const text = stringValue(item.text);
        return text
          ? [
              {
                event: AdapterEventKind.AssistantMessage,
                turn_id: turnId,
                role: MessageRole.Assistant,
                content: text,
              },
            ]
          : [];
      }
      if (item.type === "commandExecution") {
        const output = stringValue(item.aggregatedOutput);
        return output
          ? [
              {
                event: AdapterEventKind.ToolResult,
                turn_id: turnId,
                role: MessageRole.Tool,
                content: output,
                payload: item,
              },
            ]
          : [];
      }
      if (item.type === "fileChange") {
        return [
          {
            event: AdapterEventKind.ToolResult,
            turn_id: turnId,
            role: MessageRole.Tool,
            content: describeChanges(item),
            payload: item,
          },
        ];
      }
      return [];
    }
    case "thread/tokenUsage/updated": {
      const usage = usageFromTokenUsage(body.tokenUsage, tokens);
      return usage
        ? [{ event: AdapterEventKind.Usage, turn_id: turnId, usage }]
        : [];
    }
    case "error": {
      const error = asRecord(body.error);
      const message =
        stringValue(error?.message) ??
        stringValue(body.message) ??
        "Codex reported an error";
      return body.willRetry === true
        ? []
        : [
            {
              event: AdapterEventKind.Error,
              turn_id: turnId,
              content: message,
              payload: body,
            },
          ];
    }
    default:
      return [];
  }
}

/** The terminal event for a `turn/completed` notification. */
export function processTurnCompleted(
  params: unknown,
  turnId: string | undefined,
): AdapterEvent {
  const turn = asRecord(asRecord(params)?.turn);
  const status = stringValue(turn?.status);
  if (status === "completed")
    return { event: AdapterEventKind.TurnComplete, turn_id: turnId };
  const error = asRecord(turn?.error);
  const message =
    stringValue(error?.message) ??
    (status === "interrupted"
      ? "Codex turn was interrupted"
      : "Codex could not complete this turn");
  return {
    event: AdapterEventKind.Error,
    turn_id: turnId,
    content: message,
    payload: turn ?? undefined,
  };
}

/** A server-initiated request awaiting a host decision. */
export type PendingServerRequest = {
  rpcId: RpcId;
  kind: RequestKind;
  method: string;
  questionIds: string[];
};

const APPROVAL_METHODS = [
  "item/commandExecution/requestApproval",
  "item/fileChange/requestApproval",
];
const USER_INPUT_METHOD = "item/tool/requestUserInput";

/** `null` for server requests Mitos can't answer; the caller rejects those. */
export function requestFromServer(
  rpcId: RpcId,
  method: string,
  params: unknown,
  turnId: string | undefined,
): { event: AdapterEvent; pending: PendingServerRequest } | null {
  const body = asRecord(params) ?? {};
  let kind: RequestKind;
  let title: string;
  let questionIds: string[] = [];
  if (APPROVAL_METHODS.includes(method)) {
    kind = RequestKind.Permission;
    const reason = stringValue(body.reason);
    const subject =
      method === APPROVAL_METHODS[0]
        ? `Run: ${stringValue(body.command) ?? "command"}`
        : "Apply file changes";
    title = clip(reason ? `${subject} (${reason})` : subject);
  } else if (method === USER_INPUT_METHOD) {
    kind = RequestKind.Question;
    const questions = Array.isArray(body.questions)
      ? body.questions.filter(isRecord)
      : [];
    questionIds = questions.map((question) => stringValue(question.id) ?? "");
    const first = questions[0];
    const options = Array.isArray(first?.options)
      ? first.options
          .map((option) =>
            isRecord(option) ? (stringValue(option.label) ?? "") : "",
          )
          .filter(Boolean)
      : [];
    const text = stringValue(first?.question) ?? "question";
    title = clip(
      options.length > 0 ? `${text} [${options.join(" / ")}]` : text,
    );
  } else {
    return null;
  }
  const id = String(rpcId);
  const payload: RequestPayload = { id, kind, title, method, params: body };
  return {
    event: { event: AdapterEventKind.Request, turn_id: turnId, payload },
    pending: { rpcId, kind, method, questionIds },
  };
}

/** The JSON-RPC `result` answering `pending`, from the response shapes the
 * TUI sends: `{allow}` for permissions, `{answer}` for questions. */
export function resultFor(
  pending: PendingServerRequest,
  response: unknown,
): unknown {
  const answer = asRecord(response) ?? {};
  if (pending.kind === RequestKind.Question) {
    const text = stringValue(answer.answer) ?? "";
    return {
      answers: Object.fromEntries(
        pending.questionIds.map((id) => [id, { answers: [text] }]),
      ),
    };
  }
  return { decision: answer.allow === true ? "accept" : "decline" };
}

function describeChanges(item: JsonRecord): string {
  const count = Array.isArray(item.changes) ? item.changes.length : 0;
  return count === 1 ? "1 file change" : `${count} file changes`;
}

function usageFromTokenUsage(
  value: unknown,
  tokens: TurnTokens,
): HandoffUsage | undefined {
  const usage = asRecord(value);
  const total = asRecord(usage?.total);
  if (!usage || !total) return undefined;
  const last = asRecord(usage.last);
  const consumed = tokens.consumed(usage);
  return {
    input_tokens: consumed?.input,
    output_tokens: consumed?.output,
    cached_input_tokens: consumed?.cached,
    context_used_tokens: numberValue(last?.totalTokens),
    context_limit_tokens: numberValue(usage.modelContextWindow),
  };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}
