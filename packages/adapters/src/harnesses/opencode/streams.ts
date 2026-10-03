import { asRecord, isRecord, type JsonRecord } from "@json/records.ts";
import { numberValue, stringValue } from "@json/scalars.ts";
import { clip } from "@json/texts.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { RequestKind, type RequestPayload } from "@protocol/asks.ts";
import { type AdapterEvent, AdapterEventKind } from "@protocol/events.ts";
import type { HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

enum ServerEventType {
  MessageUpdated = "message.updated",
  PartUpdated = "message.part.updated",
  PartDelta = "message.part.delta",
  PermissionAsked = "permission.asked",
  QuestionAsked = "question.asked",
  SessionError = "session.error",
  SessionIdle = "session.idle",
  SessionStatus = "session.status",
}

enum PartType {
  Text = "text",
  Tool = "tool",
  StepFinish = "step-finish",
}

enum ToolStatus {
  Pending = "pending",
  Running = "running",
  Completed = "completed",
  Error = "error",
}

enum SessionStatusType {
  Idle = "idle",
}

const TEXT_FIELD = "text";

/** Plan runs the native `plan` agent; build runs `build`. */
export function agentFor(mode: ThreadMode): string {
  return mode === ThreadMode.Plan ? "plan" : "build";
}

/** A server-initiated ask awaiting a host decision. */
export type PendingAsk = {
  id: string;
  kind: RequestKind;
  questionCount: number;
};

export type AskReply = { path: string; body: JsonRecord };

export type StreamStep = { events: AdapterEvent[]; done: boolean };

const CONTINUE: StreamStep = { events: [], done: false };

/** Maps one session's `opencode serve` SSE events onto Mitos adapter events.
 * Reasoning parts are intentionally not surfaced. Exactly one step carries
 * `done`: idle, or the session error. */
export class SessionStream {
  private readonly roles = new Map<string, string>();
  private readonly partTypes = new Map<string, string>();
  private readonly finishedText = new Set<string>();
  private readonly calledTools = new Set<string>();
  private readonly resultTools = new Set<string>();
  private readonly stepTokens = new Map<string, JsonRecord>();
  private readonly stepCosts = new Map<string, number>();
  private model: string | undefined;
  private started = false;

  constructor(
    private readonly sessionId: string,
    private readonly turnId: string | undefined,
  ) {}

  /** Events from other sessions and unknown event types yield nothing. */
  process(raw: unknown): StreamStep {
    const envelope = asRecord(raw);
    const type = stringValue(envelope?.type);
    const properties = asRecord(envelope?.properties);
    if (!type || !properties) return CONTINUE;
    if (stringValue(properties.sessionID) !== this.sessionId) return CONTINUE;
    switch (type) {
      case ServerEventType.MessageUpdated:
        return this.messageUpdated(properties);
      case ServerEventType.PartUpdated:
        return this.partUpdated(properties);
      case ServerEventType.PartDelta:
        return this.partDelta(properties);
      case ServerEventType.PermissionAsked:
        this.started = true;
        return this.single(this.permissionAsked(properties));
      case ServerEventType.QuestionAsked:
        this.started = true;
        return this.single(this.questionAsked(properties));
      case ServerEventType.SessionStatus:
        return this.sessionStatus(properties);
      case ServerEventType.SessionIdle:
        return this.idle();
      case ServerEventType.SessionError:
        return {
          events: [
            {
              event: AdapterEventKind.Error,
              turn_id: this.turnId,
              content: errorMessage(properties.error),
              payload: properties.error,
            },
          ],
          done: true,
        };
      default:
        return CONTINUE;
    }
  }

  /** The pending record for a `request` event this stream produced. */
  pendingFor(event: AdapterEvent): PendingAsk | null {
    const payload = asRecord(event.payload);
    const id = stringValue(payload?.id);
    const kind = payload?.kind;
    if (!id) return null;
    if (kind === RequestKind.Permission) return { id, kind, questionCount: 0 };
    if (kind === RequestKind.Question) {
      const params = asRecord(payload?.params);
      const questions = Array.isArray(params?.questions)
        ? params.questions.length
        : 1;
      return { id, kind, questionCount: Math.max(questions, 1) };
    }
    return null;
  }

  private messageUpdated(properties: JsonRecord): StreamStep {
    this.started = true;
    const info = asRecord(properties.info);
    const id = stringValue(info?.id);
    const role = stringValue(info?.role);
    if (id && role) this.roles.set(id, role);
    if (role === MessageRole.Assistant)
      this.model = stringValue(info?.modelID) ?? this.model;
    return CONTINUE;
  }

  private partDelta(properties: JsonRecord): StreamStep {
    this.started = true;
    if (stringValue(properties.field) !== TEXT_FIELD) return CONTINUE;
    const partId = stringValue(properties.partID);
    const messageId = stringValue(properties.messageID);
    const delta = stringValue(properties.delta);
    if (!partId || !messageId || !delta) return CONTINUE;
    if (this.roles.get(messageId) !== MessageRole.Assistant) return CONTINUE;
    if (this.partTypes.get(partId) !== PartType.Text) return CONTINUE;
    return this.single({
      event: AdapterEventKind.AssistantDelta,
      turn_id: this.turnId,
      role: MessageRole.Assistant,
      content: delta,
    });
  }

  private partUpdated(properties: JsonRecord): StreamStep {
    this.started = true;
    const part = asRecord(properties.part);
    const id = stringValue(part?.id);
    const type = stringValue(part?.type);
    if (!part || !id || !type) return CONTINUE;
    this.partTypes.set(id, type);
    switch (type) {
      case PartType.Text:
        return this.single(this.textFinished(id, part));
      case PartType.Tool:
        return { events: this.toolEvents(part), done: false };
      case PartType.StepFinish:
        return this.single(this.stepFinished(id, part));
      default:
        return CONTINUE;
    }
  }

  private textFinished(id: string, part: JsonRecord): AdapterEvent | null {
    if (this.finishedText.has(id)) return null;
    if (part.synthetic === true || part.ignored === true) return null;
    const messageId = stringValue(part.messageID);
    if (!messageId || this.roles.get(messageId) !== MessageRole.Assistant)
      return null;
    if (numberValue(asRecord(part.time)?.end) === null) return null;
    const text = stringValue(part.text);
    if (!text) return null;
    this.finishedText.add(id);
    return {
      event: AdapterEventKind.AssistantMessage,
      turn_id: this.turnId,
      role: MessageRole.Assistant,
      content: text,
    };
  }

  private toolEvents(part: JsonRecord): AdapterEvent[] {
    const callId = stringValue(part.callID) ?? stringValue(part.id);
    const state = asRecord(part.state);
    const status = stringValue(state?.status);
    if (!callId || !state || !status) return [];
    const events: AdapterEvent[] = [];
    const settled =
      status === ToolStatus.Completed || status === ToolStatus.Error;
    if (
      !this.calledTools.has(callId) &&
      (status === ToolStatus.Running || settled)
    ) {
      this.calledTools.add(callId);
      events.push({
        event: AdapterEventKind.ToolCall,
        turn_id: this.turnId,
        content: describeTool(part, state),
        payload: part,
      });
    }
    if (settled && !this.resultTools.has(callId)) {
      this.resultTools.add(callId);
      const content =
        status === ToolStatus.Completed
          ? stringValue(state.output)
          : stringValue(state.error);
      events.push({
        event: AdapterEventKind.ToolResult,
        turn_id: this.turnId,
        role: MessageRole.Tool,
        content: content ?? describeTool(part, state),
        payload: part,
      });
    }
    return events;
  }

  private stepFinished(id: string, part: JsonRecord): AdapterEvent | null {
    const tokens = asRecord(part.tokens);
    if (!tokens) return null;
    this.stepTokens.set(id, tokens);
    const cost = numberValue(part.cost);
    if (cost !== null) this.stepCosts.set(id, cost);
    return {
      event: AdapterEventKind.Usage,
      turn_id: this.turnId,
      usage: this.usage(),
    };
  }

  private usage(): HandoffUsage {
    let input = 0;
    let output = 0;
    let cached = 0;
    let last: JsonRecord | undefined;
    for (const tokens of this.stepTokens.values()) {
      input += numberValue(tokens.input) ?? 0;
      output += numberValue(tokens.output) ?? 0;
      cached += numberValue(asRecord(tokens.cache)?.read) ?? 0;
      last = tokens;
    }
    const lastInput = numberValue(last?.input) ?? 0;
    const lastOutput = numberValue(last?.output) ?? 0;
    const lastCached = numberValue(asRecord(last?.cache)?.read) ?? 0;
    return {
      input_tokens: input,
      output_tokens: output,
      cached_input_tokens: cached,
      cost_usd:
        this.stepCosts.size > 0
          ? [...this.stepCosts.values()].reduce((sum, cost) => sum + cost, 0)
          : undefined,
      context_used_tokens:
        numberValue(last?.total) ?? lastInput + lastOutput + lastCached,
      model: this.model,
    };
  }

  private permissionAsked(properties: JsonRecord): AdapterEvent | null {
    const id = stringValue(properties.id);
    if (!id) return null;
    const permission = stringValue(properties.permission) ?? "permission";
    const patterns = Array.isArray(properties.patterns)
      ? properties.patterns.filter((p): p is string => typeof p === "string")
      : [];
    const title = clip(
      patterns.length > 0
        ? `${permission}: ${patterns.join(", ")}`
        : permission,
    );
    return this.request(id, RequestKind.Permission, title, properties);
  }

  private questionAsked(properties: JsonRecord): AdapterEvent | null {
    const id = stringValue(properties.id);
    if (!id) return null;
    const questions = Array.isArray(properties.questions)
      ? properties.questions.filter(isRecord)
      : [];
    const first = questions[0];
    const options = Array.isArray(first?.options)
      ? first.options
          .map((option) =>
            isRecord(option) ? (stringValue(option.label) ?? "") : "",
          )
          .filter(Boolean)
      : [];
    const text = stringValue(first?.question) ?? "question";
    const title = clip(
      options.length > 0 ? `${text} [${options.join(" / ")}]` : text,
    );
    return this.request(id, RequestKind.Question, title, properties);
  }

  private request(
    id: string,
    kind: RequestKind,
    title: string,
    params: JsonRecord,
  ): AdapterEvent {
    const payload: RequestPayload = { id, kind, title, params };
    return { event: AdapterEventKind.Request, turn_id: this.turnId, payload };
  }

  private sessionStatus(properties: JsonRecord): StreamStep {
    const status = stringValue(asRecord(properties.status)?.type);
    if (status === SessionStatusType.Idle) return this.idle();
    this.started = true;
    return CONTINUE;
  }

  /** Idle before any activity is the session's state from before this turn. */
  private idle(): StreamStep {
    if (!this.started) return CONTINUE;
    return {
      events: [{ event: AdapterEventKind.TurnComplete, turn_id: this.turnId }],
      done: true,
    };
  }

  private single(event: AdapterEvent | null): StreamStep {
    return { events: event ? [event] : [], done: false };
  }
}

/** The HTTP reply answering `pending`, from the response shapes the TUI
 * sends: `{allow}` for permissions, `{answer}` for questions. */
export function replyFor(pending: PendingAsk, response: unknown): AskReply {
  const answer = asRecord(response) ?? {};
  if (pending.kind === RequestKind.Question) {
    const text = stringValue(answer.answer) ?? "";
    return {
      path: `/question/${pending.id}/reply`,
      body: {
        answers: Array.from({ length: pending.questionCount }, () => [text]),
      },
    };
  }
  return {
    path: `/permission/${pending.id}/reply`,
    body: { reply: answer.allow === true ? "once" : "reject" },
  };
}

function errorMessage(error: unknown): string {
  const record = asRecord(error);
  const data = asRecord(record?.data);
  return (
    stringValue(data?.message) ??
    stringValue(record?.message) ??
    stringValue(record?.name) ??
    "OpenCode reported an error"
  );
}

function describeTool(part: JsonRecord, state: JsonRecord): string {
  const tool = stringValue(part.tool) ?? "tool";
  const title = stringValue(state.title);
  if (title) return clip(`${tool}: ${title}`);
  const input = asRecord(state.input);
  const detail =
    stringValue(input?.command) ??
    stringValue(input?.filePath) ??
    stringValue(input?.pattern) ??
    stringValue(input?.url);
  return clip(detail ? `${tool}: ${detail}` : tool);
}
