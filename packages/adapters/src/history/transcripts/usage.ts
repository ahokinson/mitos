import type { MinedMessage } from "@history/transcripts/messages.ts";
import { isObject } from "@json/records.ts";
import { numberValue, stringValue } from "@json/scalars.ts";
import type { HandoffUsage } from "@protocol/responses.ts";
import { MessageRole } from "@protocol/roles.ts";

/** `usageFromRecord`'s result minus `turns`, which is derived once from the
 * whole mined-message window. */
type FieldUsage = Omit<HandoffUsage, "turns">;

/** Reads one record for either harness's shape: Claude nests per-turn usage
 * under `message.usage`; Codex nests cumulative totals under
 * `payload.info.total_token_usage` and plan limits under
 * `payload.rate_limits`. `null` when the record carries neither. */
function usageFromRecord(record: unknown): FieldUsage | null {
  if (!isObject(record)) return null;
  const payload = isObject(record.payload) ? record.payload : record;
  const message = isObject(payload.message) ? payload.message : {};
  const claudeUsage = isObject(payload.usage)
    ? payload.usage
    : isObject(message.usage)
      ? message.usage
      : null;
  const model =
    stringValue(payload.model) ?? stringValue(message.model) ?? undefined;

  const info = isObject(payload.info) ? payload.info : null;
  const codexUsage = isObject(info?.total_token_usage)
    ? info.total_token_usage
    : null;
  const rateLimits = isObject(payload.rate_limits) ? payload.rate_limits : null;
  const context =
    (isObject(payload.context_window) ? payload.context_window : null) ??
    (isObject(info?.context_window) ? info.context_window : null) ??
    (isObject(payload.context) ? payload.context : null);
  const fiveHour = planWindow(rateLimits?.primary, record.timestamp);
  const week = planWindow(rateLimits?.secondary, record.timestamp);

  const inputTokens =
    numberValue(claudeUsage?.input_tokens) ??
    numberValue(codexUsage?.input_tokens) ??
    undefined;
  const outputTokens =
    numberValue(claudeUsage?.output_tokens) ??
    numberValue(codexUsage?.output_tokens) ??
    undefined;
  const cachedInputTokens =
    numberValue(claudeUsage?.cache_read_input_tokens) ??
    numberValue(claudeUsage?.cache_creation_input_tokens) ??
    numberValue(codexUsage?.cached_input_tokens) ??
    undefined;
  const contextUsedTokens =
    numberValue(context?.used_tokens) ??
    numberValue(context?.current_tokens) ??
    numberValue(context?.tokens) ??
    undefined;
  const contextLimitTokens =
    numberValue(context?.limit_tokens) ??
    numberValue(context?.max_tokens) ??
    numberValue(context?.window_tokens) ??
    undefined;

  if (
    inputTokens === undefined &&
    outputTokens === undefined &&
    cachedInputTokens === undefined &&
    contextUsedTokens === undefined &&
    contextLimitTokens === undefined &&
    !model &&
    fiveHour.percent === undefined &&
    week.percent === undefined
  ) {
    return null;
  }
  return {
    input_tokens: inputTokens,
    output_tokens: outputTokens,
    cached_input_tokens: cachedInputTokens,
    context_used_tokens: contextUsedTokens,
    context_limit_tokens: contextLimitTokens,
    model,
    plan_five_hour_percent: fiveHour.percent,
    plan_five_hour_resets_at: fiveHour.resetsAt,
    plan_week_percent: week.percent,
    plan_week_resets_at: week.resetsAt,
  };
}

/** Codex's `resets_in_seconds` is relative to the record's own timestamp, so
 * it is anchored there to keep a stale file from looking current. */
function planWindow(
  window: unknown,
  recordTimestamp: unknown,
): { percent?: number; resetsAt?: string } {
  if (!isObject(window)) return {};
  const percent = numberValue(window.used_percent) ?? undefined;
  const resetsInSeconds = numberValue(window.resets_in_seconds);
  if (percent === undefined || resetsInSeconds === null) return {};
  const anchor = Date.parse(stringValue(recordTimestamp) ?? "");
  const resetsAt = new Date(
    (Number.isFinite(anchor) ? anchor : Date.now()) + resetsInSeconds * 1000,
  ).toISOString();
  return { percent, resetsAt };
}

/** Last defined value per field wins; a spread would let a later record's
 * explicit `undefined` clobber an earlier real value. */
function mergeFieldUsage(base: FieldUsage, next: FieldUsage): FieldUsage {
  return {
    input_tokens: next.input_tokens ?? base.input_tokens,
    output_tokens: next.output_tokens ?? base.output_tokens,
    cached_input_tokens: next.cached_input_tokens ?? base.cached_input_tokens,
    context_used_tokens: next.context_used_tokens ?? base.context_used_tokens,
    context_limit_tokens:
      next.context_limit_tokens ?? base.context_limit_tokens,
    model: next.model ?? base.model,
    plan_five_hour_percent:
      next.plan_five_hour_percent ?? base.plan_five_hour_percent,
    plan_five_hour_resets_at:
      next.plan_five_hour_resets_at ?? base.plan_five_hour_resets_at,
    plan_week_percent: next.plan_week_percent ?? base.plan_week_percent,
    plan_week_resets_at: next.plan_week_resets_at ?? base.plan_week_resets_at,
  };
}

type MessageTokens = { input: number; output: number; cached: number };

/** One Claude assistant message's own usage, keyed by message id, since a
 * message can be logged on several lines. `null` for any other record. */
function claudeMessageTokens(
  record: unknown,
): { id: string; tokens: MessageTokens } | null {
  if (!isObject(record)) return null;
  const payload = isObject(record.payload) ? record.payload : record;
  const message = isObject(payload.message) ? payload.message : null;
  const id = stringValue(message?.id);
  const usage = isObject(message?.usage) ? message.usage : null;
  if (!id || !usage) return null;
  return {
    id,
    tokens: {
      input: numberValue(usage.input_tokens) ?? 0,
      output: numberValue(usage.output_tokens) ?? 0,
      cached:
        numberValue(usage.cache_read_input_tokens) ??
        numberValue(usage.cache_creation_input_tokens) ??
        0,
    },
  };
}

/** Per-field last known value, except Claude's token counts: each message
 * reports only its own call, so those are summed per distinct message. Codex
 * totals are already cumulative, so adding them would double-count. */
export function usageFromRecords(
  records: unknown[],
  minedMessages: MinedMessage[],
): HandoffUsage | null {
  let usage: FieldUsage = {};
  let found = false;
  const perMessage = new Map<string, MessageTokens>();
  for (const record of records) {
    const next = usageFromRecord(record);
    if (!next) continue;
    found = true;
    usage = mergeFieldUsage(usage, next);
    const message = claudeMessageTokens(record);
    if (message) perMessage.set(message.id, message.tokens);
  }
  if (perMessage.size > 0) {
    const sums = { input: 0, output: 0, cached: 0 };
    for (const tokens of perMessage.values()) {
      sums.input += tokens.input;
      sums.output += tokens.output;
      sums.cached += tokens.cached;
    }
    usage = {
      ...usage,
      input_tokens: sums.input,
      output_tokens: sums.output,
      cached_input_tokens: sums.cached,
    };
  }
  const turns = minedMessages.filter(
    (message) => message.role === MessageRole.Assistant,
  ).length;
  if (!found) return turns > 0 ? { turns } : null;
  return { ...usage, turns };
}
