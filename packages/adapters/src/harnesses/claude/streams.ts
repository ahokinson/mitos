import { isRecord } from "@json/records.ts";
import { clip } from "@json/texts.ts";
import { RequestKind, type RequestPayload } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";
import type { HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

enum BlockType {
  Text = "text",
  ToolUse = "tool_use",
  ToolResult = "tool_result",
}

type ClaudeContentBlock =
  | { type: BlockType.Text; text: string }
  | { type: BlockType.ToolUse; id: string; name: string; input: unknown }
  | { type: BlockType.ToolResult; tool_use_id: string; content: unknown }
  | { type: string; [key: string]: unknown };

/** Translates one line of `claude -p --output-format stream-json --verbose`
 * output into zero or more `AdapterEvent`s. Pure: no I/O, so it's testable
 * against synthetic input without spawning the real `claude` binary. */
export function processStreamLine(
  record: unknown,
  turnId: string | undefined,
): AdapterEvent[] {
  if (!isRecord(record)) return [];
  const events: AdapterEvent[] = [];
  const sessionId =
    typeof record.session_id === "string" ? record.session_id : null;
  if (sessionId) {
    events.push({
      event: AdapterEventKind.NativeSessionUpdate,
      turn_id: turnId,
      native_session: sessionId,
    });
  }
  if (record.parent_tool_use_id) return events; // subagent chatter, not this turn's own

  switch (record.type) {
    case "assistant": {
      const message = isRecord(record.message) ? record.message : {};
      for (const block of asContentBlocks(message.content)) {
        if (block.type === BlockType.Text && typeof block.text === "string") {
          events.push({
            event: AdapterEventKind.AssistantMessage,
            turn_id: turnId,
            role: MessageRole.Assistant,
            content: block.text,
          });
        } else if (
          block.type === BlockType.ToolUse &&
          typeof block.name === "string"
        ) {
          events.push({
            event: AdapterEventKind.ToolCall,
            turn_id: turnId,
            role: MessageRole.Assistant,
            content: block.name,
            payload: block,
          });
        }
      }
      if (isRecord(message.usage)) {
        events.push({
          event: AdapterEventKind.Usage,
          turn_id: turnId,
          usage: usageFromClaudeUsage(message.usage, message.model),
        });
      }
      break;
    }
    case "user": {
      const message = isRecord(record.message) ? record.message : {};
      for (const block of asContentBlocks(message.content)) {
        if (block.type === BlockType.ToolResult) {
          events.push({
            event: AdapterEventKind.ToolResult,
            turn_id: turnId,
            role: MessageRole.Tool,
            content: textFromToolResultContent(block.content),
            payload: block,
          });
        }
      }
      break;
    }
    case "result": {
      const cost =
        typeof record.total_cost_usd === "number"
          ? record.total_cost_usd
          : undefined;
      if (isRecord(record.usage) || cost !== undefined) {
        events.push({
          event: AdapterEventKind.Usage,
          turn_id: turnId,
          usage: {
            ...usageFromClaudeUsage(isRecord(record.usage) ? record.usage : {}),
            cost_usd: cost,
          },
        });
      }
      events.push({ event: AdapterEventKind.TurnComplete, turn_id: turnId });
      break;
    }
    default:
      break;
  }
  return events;
}

/** A `can_use_tool` control request awaiting a host decision. */
export type PendingControl = {
  cliRequestId: string;
  kind: RequestKind;
  toolName: string;
  input: Record<string, unknown>;
};

function questionsOf(
  input: Record<string, unknown>,
): Array<Record<string, unknown>> {
  return Array.isArray(input.questions) ? input.questions.filter(isRecord) : [];
}

function titleFor(
  kind: RequestKind,
  toolName: string,
  input: Record<string, unknown>,
): string {
  switch (kind) {
    case RequestKind.PlanApproval:
      return clip(typeof input.plan === "string" ? input.plan : "plan");
    case RequestKind.Question: {
      const [first] = questionsOf(input);
      const text =
        typeof first?.question === "string" ? first.question : "question";
      const options = Array.isArray(first?.options)
        ? first.options
            .map((option) =>
              isRecord(option) && typeof option.label === "string"
                ? option.label
                : "",
            )
            .filter(Boolean)
        : [];
      return clip(
        options.length > 0 ? `${text} [${options.join(" / ")}]` : text,
      );
    }
    case RequestKind.Permission: {
      const detail =
        typeof input.command === "string"
          ? input.command
          : JSON.stringify(input);
      return clip(`${toolName} ${detail}`);
    }
  }
}

/** Translates a `control_request` / `can_use_tool` line into an adapter
 * `request` event, plus what's needed to answer it later. `null` for any
 * other control traffic. */
export function requestFromControl(
  record: unknown,
  turnId: string | undefined,
): { event: AdapterEvent; pending: PendingControl } | null {
  if (!isRecord(record) || record.type !== "control_request") return null;
  const request = isRecord(record.request) ? record.request : null;
  if (request?.subtype !== "can_use_tool") return null;
  if (
    typeof record.request_id !== "string" ||
    typeof request.tool_name !== "string"
  )
    return null;

  const toolName = request.tool_name;
  const input = isRecord(request.input) ? request.input : {};
  const kind =
    toolName === "ExitPlanMode"
      ? RequestKind.PlanApproval
      : toolName === "AskUserQuestion"
        ? RequestKind.Question
        : RequestKind.Permission;
  const payload: RequestPayload = {
    id: record.request_id,
    kind,
    title: titleFor(kind, toolName, input),
    tool_name: toolName,
    input,
  };
  return {
    event: { event: AdapterEventKind.Request, turn_id: turnId, payload },
    pending: { cliRequestId: record.request_id, kind, toolName, input },
  };
}

function textOf(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

/** The `control_response` line answering `pending`, from the response shapes
 * the TUI sends: `{allow, message?}`, `{approved, feedback?}`, `{answer}`. */
export function controlResponseFor(
  pending: PendingControl,
  response: unknown,
): Record<string, unknown> {
  const answer = isRecord(response) ? response : {};
  let body: Record<string, unknown>;
  switch (pending.kind) {
    case RequestKind.Permission:
      body =
        answer.allow === true
          ? { behavior: "allow", updatedInput: pending.input }
          : {
              behavior: "deny",
              message: textOf(answer.message) ?? "Denied by the user.",
            };
      break;
    case RequestKind.PlanApproval:
      body =
        answer.approved === true
          ? { behavior: "allow", updatedInput: pending.input }
          : {
              behavior: "deny",
              message: textOf(answer.feedback) ?? "The user rejected the plan.",
            };
      break;
    case RequestKind.Question: {
      const [first] = questionsOf(pending.input);
      const question =
        typeof first?.question === "string" ? first.question : "question";
      body = {
        behavior: "allow",
        updatedInput: {
          ...pending.input,
          answers: { [question]: textOf(answer.answer) ?? "" },
        },
      };
      break;
    }
  }
  return {
    type: "control_response",
    response: {
      subtype: "success",
      request_id: pending.cliRequestId,
      response: body,
    },
  };
}

function asContentBlocks(value: unknown): ClaudeContentBlock[] {
  return Array.isArray(value) ? (value as ClaudeContentBlock[]) : [];
}

function textFromToolResultContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) =>
        isRecord(part) && typeof part.text === "string" ? part.text : "",
      )
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function usageFromClaudeUsage(
  usage: Record<string, unknown>,
  model?: unknown,
): HandoffUsage {
  return {
    input_tokens: numberOrUndefined(usage.input_tokens),
    output_tokens: numberOrUndefined(usage.output_tokens),
    cached_input_tokens:
      numberOrUndefined(usage.cache_read_input_tokens) ??
      numberOrUndefined(usage.cache_creation_input_tokens),
    model: typeof model === "string" ? model : undefined,
  };
}

function numberOrUndefined(value: unknown): number | undefined {
  return typeof value === "number" ? value : undefined;
}
