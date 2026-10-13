use serde_json::Value;

use super::super::tokens::TurnTokens;
use super::event;
use crate::json::{Record, count_value, string_value};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::responses::CollectedUsage;

const ASSISTANT: &str = "assistant";
const TOOL: &str = "tool";

fn describe_changes(item: &Record) -> String {
    match item.get("changes").and_then(Value::as_array).map(Vec::len) {
        Some(1) => "1 file change".into(),
        count => format!("{} file changes", count.unwrap_or(0)),
    }
}

fn item_payload(item: &Record) -> Value {
    Value::Object(item.clone())
}

/// Maps `codex app-server` notifications onto Mitos's streaming adapter
/// events. Reasoning items are intentionally not surfaced. `turn/completed`
/// is not handled here: the caller owns the end of the turn.
pub fn process_notification(
    method: &str,
    params: &Value,
    tokens: &mut TurnTokens,
) -> Vec<AdapterEvent> {
    let Some(body) = params.as_object() else {
        return Vec::new();
    };
    let item = body.get("item").and_then(Value::as_object);
    let item_type = item
        .and_then(|item| item.get("type"))
        .and_then(Value::as_str);
    match (method, item, item_type) {
        ("item/agentMessage/delta", _, _) => string_value(body.get("delta"))
            .map(|delta| {
                event(
                    kinds::ASSISTANT_DELTA,
                    Some(ASSISTANT),
                    Some(delta.into()),
                    None,
                )
            })
            .into_iter()
            .collect(),
        ("item/started", Some(item), Some("commandExecution")) => string_value(item.get("command"))
            .map(|command| {
                event(
                    kinds::TOOL_CALL,
                    None,
                    Some(command.into()),
                    Some(item_payload(item)),
                )
            })
            .into_iter()
            .collect(),
        ("item/started", Some(item), Some("fileChange")) => vec![event(
            kinds::TOOL_CALL,
            None,
            Some(describe_changes(item)),
            Some(item_payload(item)),
        )],
        ("item/completed", Some(item), Some("agentMessage" | "plan")) => {
            string_value(item.get("text"))
                .map(|text| {
                    event(
                        kinds::ASSISTANT_MESSAGE,
                        Some(ASSISTANT),
                        Some(text.into()),
                        None,
                    )
                })
                .into_iter()
                .collect()
        }
        ("item/completed", Some(item), Some("commandExecution")) => {
            string_value(item.get("aggregatedOutput"))
                .map(|output| {
                    event(
                        kinds::TOOL_RESULT,
                        Some(TOOL),
                        Some(output.into()),
                        Some(item_payload(item)),
                    )
                })
                .into_iter()
                .collect()
        }
        ("item/completed", Some(item), Some("fileChange")) => vec![event(
            kinds::TOOL_RESULT,
            Some(TOOL),
            Some(describe_changes(item)),
            Some(item_payload(item)),
        )],
        ("thread/tokenUsage/updated", _, _) => {
            usage_from_token_usage(body.get("tokenUsage"), tokens)
                .map(|usage| AdapterEvent {
                    usage: Some(usage),
                    ..AdapterEvent::new(kinds::USAGE)
                })
                .into_iter()
                .collect()
        }
        ("error", _, _) => {
            if body.get("willRetry") == Some(&Value::Bool(true)) {
                return Vec::new();
            }
            let message = string_value(body.get("error").and_then(|error| error.get("message")))
                .or_else(|| string_value(body.get("message")))
                .unwrap_or("Codex reported an error");
            vec![event(
                kinds::ERROR,
                None,
                Some(message.into()),
                Some(params.clone()),
            )]
        }
        _ => Vec::new(),
    }
}

fn usage_from_token_usage(
    value: Option<&Value>,
    tokens: &mut TurnTokens,
) -> Option<CollectedUsage> {
    let value = value?;
    let usage = value.as_object()?;
    usage.get("total")?.as_object()?;
    let last = usage.get("last").and_then(Value::as_object);
    let consumed = tokens.consumed(value);
    Some(CollectedUsage {
        input_tokens: consumed.map(|counts| counts.input),
        output_tokens: consumed.map(|counts| counts.output),
        cached_input_tokens: consumed.map(|counts| counts.cached),
        context_used_tokens: count_value(last.and_then(|last| last.get("totalTokens"))),
        context_limit_tokens: count_value(usage.get("modelContextWindow")),
        ..CollectedUsage::default()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn usage(input: u64, output: u64, cached: u64) -> Value {
        json!({
            "total": { "inputTokens": input, "outputTokens": output, "cachedInputTokens": cached },
            "last": { "totalTokens": 64_000 },
            "modelContextWindow": 200_000
        })
    }

    #[test]
    fn token_usage_collects_turn_consumption_and_context() {
        let mut tokens = TurnTokens::default();
        let early = usage_from_token_usage(Some(&usage(10, 2, 1)), &mut tokens).unwrap();
        assert!(early.input_tokens.is_none());
        assert_eq!(early.context_used_tokens, Some(64_000));
        assert_eq!(early.context_limit_tokens, Some(200_000));

        tokens.begin();
        let live = usage_from_token_usage(Some(&usage(15, 4, 1)), &mut tokens).unwrap();
        assert_eq!(live.input_tokens, Some(5));
        assert_eq!(live.output_tokens, Some(2));
        assert_eq!(live.cached_input_tokens, Some(0));

        assert!(usage_from_token_usage(None, &mut tokens).is_none());
        assert!(usage_from_token_usage(Some(&json!("nope")), &mut tokens).is_none());
    }

    #[test]
    fn a_fresh_turn_baselines_from_the_first_total_minus_its_last() {
        let mut tokens = TurnTokens::default();
        tokens.begin();
        let first = usage_from_token_usage(
            Some(&json!({
                "total": { "inputTokens": 10, "outputTokens": 2, "cachedInputTokens": 1 },
                "last": { "inputTokens": 4, "outputTokens": 1, "cachedInputTokens": 1 }
            })),
            &mut tokens,
        )
        .unwrap();
        assert_eq!(first.input_tokens, Some(4));
        assert_eq!(first.output_tokens, Some(1));

        let second = usage_from_token_usage(Some(&usage(12, 3, 1)), &mut tokens).unwrap();
        assert_eq!(second.input_tokens, Some(6));
        assert_eq!(second.output_tokens, Some(2));
    }
}
