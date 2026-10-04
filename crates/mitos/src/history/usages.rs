use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::messages::{ASSISTANT, payload_of};
use crate::json::{Record, count_value, field, number_value, object_field, string_value};
use crate::wire::responses::{CollectedMessage, CollectedUsage};

/// Reads one record for either harness's shape: Claude nests per-turn usage
/// under `message.usage`; Codex nests cumulative totals under
/// `payload.info.total_token_usage` and plan limits under `payload.rate_limits`.
fn usage_from_record(record: &Value) -> Option<CollectedUsage> {
    let record = record.as_object()?;
    let payload = payload_of(record);
    let message = object_field(Some(payload), "message");
    let claude_usage =
        object_field(Some(payload), "usage").or_else(|| object_field(message, "usage"));
    let model = string_value(field(Some(payload), "model"))
        .or_else(|| string_value(field(message, "model")))
        .map(str::to_string);

    let info = object_field(Some(payload), "info");
    let codex_usage = object_field(info, "total_token_usage");
    let rate_limits = object_field(Some(payload), "rate_limits");
    let context = object_field(Some(payload), "context_window")
        .or_else(|| object_field(info, "context_window"))
        .or_else(|| object_field(Some(payload), "context"));
    let timestamp = record.get("timestamp");
    let five_hour = plan_window(field(rate_limits, "primary"), timestamp);
    let week = plan_window(field(rate_limits, "secondary"), timestamp);

    let usage = CollectedUsage {
        input_tokens: count_value(field(claude_usage, "input_tokens"))
            .or_else(|| count_value(field(codex_usage, "input_tokens"))),
        output_tokens: count_value(field(claude_usage, "output_tokens"))
            .or_else(|| count_value(field(codex_usage, "output_tokens"))),
        cached_input_tokens: count_value(field(claude_usage, "cache_read_input_tokens"))
            .or_else(|| count_value(field(claude_usage, "cache_creation_input_tokens")))
            .or_else(|| count_value(field(codex_usage, "cached_input_tokens"))),
        cost_usd: None,
        context_used_tokens: ["used_tokens", "current_tokens", "tokens"]
            .into_iter()
            .find_map(|key| count_value(field(context, key))),
        context_limit_tokens: ["limit_tokens", "max_tokens", "window_tokens"]
            .into_iter()
            .find_map(|key| count_value(field(context, key))),
        model,
        turns: None,
        plan_five_hour_percent: five_hour.as_ref().map(|window| window.0),
        plan_five_hour_resets_at: five_hour.map(|window| window.1),
        plan_week_percent: week.as_ref().map(|window| window.0),
        plan_week_resets_at: week.map(|window| window.1),
    };
    has_any_field(&usage).then_some(usage)
}

fn has_any_field(usage: &CollectedUsage) -> bool {
    usage.input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.cached_input_tokens.is_some()
        || usage.context_used_tokens.is_some()
        || usage.context_limit_tokens.is_some()
        || usage.model.is_some()
        || usage.plan_five_hour_percent.is_some()
        || usage.plan_week_percent.is_some()
}

/// Codex's `resets_in_seconds` is relative to the record's own timestamp, so
/// it is anchored there to keep a stale file from looking current.
#[allow(clippy::cast_possible_truncation)]
fn plan_window(window: Option<&Value>, record_timestamp: Option<&Value>) -> Option<(f64, String)> {
    let window = window?.as_object()?;
    let percent = number_value(window.get("used_percent"))?;
    let resets_in_seconds = number_value(window.get("resets_in_seconds"))?;
    let anchor = string_value(record_timestamp)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map_or_else(Utc::now, |parsed| parsed.with_timezone(&Utc));
    let resets_at = anchor + chrono::Duration::milliseconds((resets_in_seconds * 1000.0) as i64);
    Some((
        percent,
        resets_at.to_rfc3339_opts(SecondsFormat::Millis, true),
    ))
}

/// Last defined value per field wins.
fn merge(base: CollectedUsage, next: CollectedUsage) -> CollectedUsage {
    CollectedUsage {
        input_tokens: next.input_tokens.or(base.input_tokens),
        output_tokens: next.output_tokens.or(base.output_tokens),
        cached_input_tokens: next.cached_input_tokens.or(base.cached_input_tokens),
        cost_usd: next.cost_usd.or(base.cost_usd),
        context_used_tokens: next.context_used_tokens.or(base.context_used_tokens),
        context_limit_tokens: next.context_limit_tokens.or(base.context_limit_tokens),
        model: next.model.or(base.model),
        turns: next.turns.or(base.turns),
        plan_five_hour_percent: next.plan_five_hour_percent.or(base.plan_five_hour_percent),
        plan_five_hour_resets_at: next
            .plan_five_hour_resets_at
            .or(base.plan_five_hour_resets_at),
        plan_week_percent: next.plan_week_percent.or(base.plan_week_percent),
        plan_week_resets_at: next.plan_week_resets_at.or(base.plan_week_resets_at),
    }
}

#[derive(Default)]
struct MessageTokens {
    input: u64,
    output: u64,
    cached: u64,
}

/// One Claude assistant message's own usage, keyed by message id, since a
/// message can be logged on several lines.
fn claude_message_tokens(record: &Value) -> Option<(String, MessageTokens)> {
    let payload: &Record = payload_of(record.as_object()?);
    let message = object_field(Some(payload), "message");
    let id = string_value(field(message, "id"))?;
    let usage = object_field(message, "usage")?;
    let count = |key: &str| count_value(usage.get(key));
    Some((
        id.to_string(),
        MessageTokens {
            input: count("input_tokens").unwrap_or(0),
            output: count("output_tokens").unwrap_or(0),
            cached: count("cache_read_input_tokens")
                .or_else(|| count("cache_creation_input_tokens"))
                .unwrap_or(0),
        },
    ))
}

/// Per-field last known value, except Claude's token counts: each message
/// reports only its own call, so those are summed per distinct message. Codex
/// totals are already cumulative, so adding them would double-count.
pub fn usage_from_records(records: &[Value], mined: &[CollectedMessage]) -> Option<CollectedUsage> {
    let mut usage = CollectedUsage::default();
    let mut found = false;
    let mut per_message: HashMap<String, MessageTokens> = HashMap::new();
    for record in records {
        let Some(next) = usage_from_record(record) else {
            continue;
        };
        found = true;
        usage = merge(usage, next);
        if let Some((id, tokens)) = claude_message_tokens(record) {
            per_message.insert(id, tokens);
        }
    }
    if !per_message.is_empty() {
        let (mut input, mut output, mut cached) = (0, 0, 0);
        for tokens in per_message.values() {
            input += tokens.input;
            output += tokens.output;
            cached += tokens.cached;
        }
        usage.input_tokens = Some(input);
        usage.output_tokens = Some(output);
        usage.cached_input_tokens = Some(cached);
    }
    let turns = u32::try_from(
        mined
            .iter()
            .filter(|message| message.role == ASSISTANT)
            .count(),
    )
    .unwrap_or(u32::MAX);
    if !found {
        return (turns > 0).then(|| CollectedUsage {
            turns: Some(turns),
            ..CollectedUsage::default()
        });
    }
    usage.turns = Some(turns);
    Some(usage)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn assistant(text: &str) -> CollectedMessage {
        CollectedMessage {
            role: ASSISTANT.into(),
            text: text.into(),
        }
    }

    #[test]
    fn claude_tokens_sum_per_distinct_message_id() {
        let records = vec![
            json!({ "message": { "id": "m1", "model": "sonnet", "usage": { "input_tokens": 10, "output_tokens": 2 } } }),
            json!({ "message": { "id": "m1", "usage": { "input_tokens": 10, "output_tokens": 4 } } }),
            json!({ "message": { "id": "m2", "usage": { "input_tokens": 5, "output_tokens": 1, "cache_read_input_tokens": 3 } } }),
        ];
        let usage = usage_from_records(&records, &[assistant("a"), assistant("b")]).unwrap();
        assert_eq!(usage.input_tokens, Some(15));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.cached_input_tokens, Some(3));
        assert_eq!(usage.model.as_deref(), Some("sonnet"));
        assert_eq!(usage.turns, Some(2));
    }

    #[test]
    fn codex_totals_are_taken_as_the_last_value_not_summed() {
        let records = vec![
            json!({ "payload": { "info": { "total_token_usage": { "input_tokens": 100, "output_tokens": 10 } } } }),
            json!({ "payload": { "info": { "total_token_usage": { "input_tokens": 250, "output_tokens": 30, "cached_input_tokens": 50 } } } }),
        ];
        let usage = usage_from_records(&records, &[]).unwrap();
        assert_eq!(usage.input_tokens, Some(250));
        assert_eq!(usage.output_tokens, Some(30));
        assert_eq!(usage.cached_input_tokens, Some(50));
    }

    #[test]
    fn plan_windows_anchor_on_the_record_timestamp() {
        let records = vec![json!({
            "timestamp": "2026-01-01T00:00:00.000Z",
            "payload": { "rate_limits": { "primary": { "used_percent": 42.5, "resets_in_seconds": 60 } } }
        })];
        let usage = usage_from_records(&records, &[]).unwrap();
        assert_eq!(usage.plan_five_hour_percent, Some(42.5));
        assert_eq!(
            usage.plan_five_hour_resets_at.as_deref(),
            Some("2026-01-01T00:01:00.000Z")
        );
        assert_eq!(usage.plan_week_percent, None);
    }

    #[test]
    fn no_usage_records_fall_back_to_turn_count_or_nothing() {
        let records = vec![json!({ "role": "user" })];
        assert!(usage_from_records(&records, &[]).is_none());
        let usage = usage_from_records(&records, &[assistant("a")]).unwrap();
        assert_eq!(usage.turns, Some(1));
        assert_eq!(usage.input_tokens, None);
    }
}
