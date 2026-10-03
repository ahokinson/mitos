import { asRecord, isRecord, type JsonRecord } from "@json/records.ts";
import { numberValue, stringValue } from "@json/scalars.ts";
import { clip } from "@json/texts.ts";
import { RequestKind, type RequestPayload } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";
import type { HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";
import type { RpcId } from "@rpc/peers.ts";

export const SESSION_UPDATE = "session/update";
export const REQUEST_PERMISSION = "session/request_permission";

enum UpdateKind {
  AgentMessageChunk = "agent_message_chunk",
  ToolCall = "tool_call",
  ToolCallUpdate = "tool_call_update",
  UsageUpdate = "usage_update",
}

enum ToolStatus {
  Completed = "completed",
  Failed = "failed",
}

enum StopReason {
  EndTurn = "end_turn",
  MaxTokens = "max_tokens",
  MaxTurnRequests = "max_turn_requests",
  Refusal = "refusal",
  Cancelled = "cancelled",
}

enum OptionKind {
  AllowOnce = "allow_once",
  AllowAlways = "allow_always",
  RejectOnce = "reject_once",
  RejectAlways = "reject_always",
}

/** A permission request awaiting a host decision. */
export type PendingPermission = {
  rpcId: RpcId;
  options: Array<{ optionId: string; kind: string }>;
};

/** Maps one ACP session's `session/update` notifications onto Mitos adapter
 * events. ACP streams assistant text only as chunks, so the text is also
 * emitted whole as one message before each tool call and at the end of the
 * turn. Reasoning chunks are intentionally not surfaced. */
export class AcpStream {
  private text = "";
  private readonly called = new Set<string>();
  private readonly resolved = new Set<string>();

  constructor(
    private readonly sessionId: string,
    private readonly turnId: string | undefined,
  ) {}

  /** Updates for other sessions yield nothing. */
  update(params: unknown): AdapterEvent[] {
    const body = asRecord(params);
    if (stringValue(body?.sessionId) !== this.sessionId) return [];
    const update = asRecord(body?.update);
    switch (stringValue(update?.sessionUpdate)) {
      case UpdateKind.AgentMessageChunk:
        return this.messageChunk(update);
      case UpdateKind.ToolCall:
        return this.toolCall(update);
      case UpdateKind.ToolCallUpdate:
        return this.toolCallUpdate(update);
      case UpdateKind.UsageUpdate:
        return this.usageUpdate(update);
      default:
        return [];
    }
  }

  /** The terminal events for a finished `session/prompt`. */
  finish(result: unknown): AdapterEvent[] {
    const body = asRecord(result);
    const events = this.flush();
    const usage = usageFromResult(body?.usage);
    if (usage)
      events.push({
        event: AdapterEventKind.Usage,
        turn_id: this.turnId,
        usage,
      });
    const stop = stringValue(body?.stopReason) ?? StopReason.EndTurn;
    if (stop === StopReason.EndTurn) {
      events.push({
        event: AdapterEventKind.TurnComplete,
        turn_id: this.turnId,
      });
    } else {
      events.push({
        event: AdapterEventKind.Error,
        turn_id: this.turnId,
        content: stopMessage(stop),
        payload: body ?? undefined,
      });
    }
    return events;
  }

  private messageChunk(update: JsonRecord | null): AdapterEvent[] {
    const content = asRecord(update?.content);
    const text = content?.type === "text" ? stringValue(content.text) : null;
    if (!text) return [];
    this.text += text;
    return [
      {
        event: AdapterEventKind.AssistantDelta,
        turn_id: this.turnId,
        role: MessageRole.Assistant,
        content: text,
      },
    ];
  }

  private toolCall(update: JsonRecord | null): AdapterEvent[] {
    const id = stringValue(update?.toolCallId);
    if (!update || !id || this.called.has(id)) return [];
    this.called.add(id);
    return [
      ...this.flush(),
      {
        event: AdapterEventKind.ToolCall,
        turn_id: this.turnId,
        content: clip(stringValue(update.title) ?? "tool"),
        payload: update,
      },
    ];
  }

  private toolCallUpdate(update: JsonRecord | null): AdapterEvent[] {
    const id = stringValue(update?.toolCallId);
    const status = stringValue(update?.status);
    if (!update || !id || this.resolved.has(id)) return [];
    if (status !== ToolStatus.Completed && status !== ToolStatus.Failed)
      return [];
    this.resolved.add(id);
    return [
      {
        event: AdapterEventKind.ToolResult,
        turn_id: this.turnId,
        role: MessageRole.Tool,
        content: toolOutput(update) ?? stringValue(update.title) ?? status,
        payload: update,
      },
    ];
  }

  private usageUpdate(update: JsonRecord | null): AdapterEvent[] {
    const used = numberValue(update?.used);
    const size = numberValue(update?.size);
    const cost = asRecord(update?.cost);
    const costUsd =
      stringValue(cost?.currency) === "USD" ? numberValue(cost?.amount) : null;
    if (used === null && size === null && costUsd === null) return [];
    return [
      {
        event: AdapterEventKind.Usage,
        turn_id: this.turnId,
        usage: {
          context_used_tokens: used ?? undefined,
          context_limit_tokens: size ?? undefined,
          cost_usd: costUsd ?? undefined,
        },
      },
    ];
  }

  private flush(): AdapterEvent[] {
    const text = this.text.trim();
    this.text = "";
    return text
      ? [
          {
            event: AdapterEventKind.AssistantMessage,
            turn_id: this.turnId,
            role: MessageRole.Assistant,
            content: text,
          },
        ]
      : [];
  }
}

/** `null` when the request has nothing the host could decide. */
export function permissionRequest(
  rpcId: RpcId,
  params: unknown,
  turnId: string | undefined,
): { event: AdapterEvent; pending: PendingPermission } | null {
  const body = asRecord(params) ?? {};
  const options = (Array.isArray(body.options) ? body.options : [])
    .filter(isRecord)
    .flatMap((option) => {
      const optionId = stringValue(option.optionId);
      return optionId
        ? [{ optionId, kind: stringValue(option.kind) ?? "" }]
        : [];
    });
  if (options.length === 0) return null;
  const title = clip(
    stringValue(asRecord(body.toolCall)?.title) ?? "Permission requested",
  );
  const payload: RequestPayload = {
    id: String(rpcId),
    kind: RequestKind.Permission,
    title,
    method: REQUEST_PERMISSION,
    params: body,
  };
  return {
    event: { event: AdapterEventKind.Request, turn_id: turnId, payload },
    pending: { rpcId, options },
  };
}

/** The `session/request_permission` result for the TUI's `{allow}` answer. */
export function outcomeFor(
  pending: PendingPermission,
  response: unknown,
): unknown {
  const allow = asRecord(response)?.allow === true;
  const preferred = allow
    ? [OptionKind.AllowOnce, OptionKind.AllowAlways]
    : [OptionKind.RejectOnce, OptionKind.RejectAlways];
  for (const kind of preferred) {
    const match = pending.options.find((option) => option.kind === kind);
    if (match)
      return { outcome: { outcome: "selected", optionId: match.optionId } };
  }
  return { outcome: { outcome: "cancelled" } };
}

function toolOutput(update: JsonRecord): string | undefined {
  const blocks = Array.isArray(update.content) ? update.content : [];
  const text = blocks
    .map((block) => {
      const inner = asRecord(asRecord(block)?.content);
      return inner?.type === "text" ? stringValue(inner.text) : null;
    })
    .filter(Boolean)
    .join("\n");
  if (text) return text;
  const raw = update.rawOutput;
  if (typeof raw === "string" && raw) return raw;
  return raw === undefined ? undefined : clip(JSON.stringify(raw));
}

function usageFromResult(value: unknown): HandoffUsage | undefined {
  const usage = asRecord(value);
  if (!usage) return undefined;
  const input = numberValue(usage.inputTokens);
  const output = numberValue(usage.outputTokens);
  const cached = numberValue(usage.cachedReadTokens);
  if (input === null && output === null && cached === null) return undefined;
  return {
    input_tokens: input ?? undefined,
    output_tokens: output ?? undefined,
    cached_input_tokens: cached ?? undefined,
  };
}

function stopMessage(stop: string): string {
  switch (stop) {
    case StopReason.Cancelled:
      return "Hermes turn was cancelled";
    case StopReason.Refusal:
      return "Hermes refused this turn";
    case StopReason.MaxTokens:
      return "Hermes stopped at its token limit";
    case StopReason.MaxTurnRequests:
      return "Hermes stopped at its request limit for the turn";
    default:
      return `Hermes stopped: ${stop}`;
  }
}
