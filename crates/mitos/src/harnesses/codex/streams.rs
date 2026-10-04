use serde_json::Value;

use super::tokens::TurnTokens;
use crate::json::{Record, count_value, string_value};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::responses::CollectedUsage;

const ASSISTANT: &str = "assistant";
const TOOL: &str = "tool";

fn event(
    kind: &str,
    role: Option<&str>,
    content: Option<String>,
    payload: Option<Value>,
) -> AdapterEvent {
    AdapterEvent {
        role: role.map(str::to_string),
        content,
        payload,
        ..AdapterEvent::new(kind)
    }
}

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

/// The terminal event for a `turn/completed` notification.
pub fn process_turn_completed(params: &Value) -> AdapterEvent {
    let turn = params.get("turn").and_then(Value::as_object);
    let status = string_value(turn.and_then(|turn| turn.get("status")));
    if status == Some("completed") {
        return AdapterEvent::new(kinds::TURN_COMPLETE);
    }
    let fallback = if status == Some("interrupted") {
        "Codex turn was interrupted"
    } else {
        "Codex could not complete this turn"
    };
    let message = string_value(
        turn.and_then(|turn| turn.get("error"))
            .and_then(|error| error.get("message")),
    )
    .unwrap_or(fallback);
    event(
        kinds::ERROR,
        None,
        Some(message.into()),
        turn.map(|turn| Value::Object(turn.clone())),
    )
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
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::harnesses::codex::tokens::Counts;

    fn run(method: &str, params: Value) -> Vec<AdapterEvent> {
        process_notification(method, &params, &mut TurnTokens::default())
    }

    #[test]
    fn agent_message_deltas_and_completed_items_become_assistant_events() {
        let delta = run(
            "item/agentMessage/delta",
            json!({ "delta": "Hel", "itemId": "i" }),
        );
        assert_eq!(delta[0].event, kinds::ASSISTANT_DELTA);
        assert_eq!(delta[0].role.as_deref(), Some("assistant"));
        assert_eq!(delta[0].content.as_deref(), Some("Hel"));

        let done = run(
            "item/completed",
            json!({ "item": { "type": "agentMessage", "id": "i", "text": "Done." } }),
        );
        assert_eq!(done[0].event, kinds::ASSISTANT_MESSAGE);
        assert_eq!(done[0].content.as_deref(), Some("Done."));

        let plan = run(
            "item/completed",
            json!({ "item": { "type": "plan", "text": "1. x" } }),
        );
        assert_eq!(plan[0].event, kinds::ASSISTANT_MESSAGE);
    }

    #[test]
    fn command_executions_surface_as_tool_calls_and_results() {
        let item = json!({ "type": "commandExecution", "id": "c", "command": "ls", "aggregatedOutput": "a\nb" });
        let call = run("item/started", json!({ "item": item }));
        assert_eq!(call[0].event, kinds::TOOL_CALL);
        assert_eq!(call[0].content.as_deref(), Some("ls"));
        assert_eq!(call[0].role, None);
        assert_eq!(call[0].payload, Some(item.clone()));

        let result = run("item/completed", json!({ "item": item }));
        assert_eq!(result[0].event, kinds::TOOL_RESULT);
        assert_eq!(result[0].role.as_deref(), Some("tool"));
        assert_eq!(result[0].content.as_deref(), Some("a\nb"));
        assert_eq!(result[0].payload, Some(item));
    }

    #[test]
    fn file_changes_are_counted() {
        let one = run(
            "item/started",
            json!({ "item": { "type": "fileChange", "changes": [{}] } }),
        );
        assert_eq!(one[0].content.as_deref(), Some("1 file change"));
        let many = run(
            "item/completed",
            json!({ "item": { "type": "fileChange", "changes": [{}, {}] } }),
        );
        assert_eq!(many[0].content.as_deref(), Some("2 file changes"));
        assert_eq!(many[0].event, kinds::TOOL_RESULT);
    }

    #[test]
    fn reasoning_items_do_not_enter_the_conversation() {
        assert!(
            run(
                "item/completed",
                json!({ "item": { "type": "reasoning", "id": "r" } })
            )
            .is_empty()
        );
    }

    fn breakdown(input: u64, output: u64, cached: u64, total: u64) -> Value {
        json!({
            "inputTokens": input,
            "outputTokens": output,
            "cachedInputTokens": cached,
            "reasoningOutputTokens": 0,
            "totalTokens": total
        })
    }

    fn token_usage(total: Value, last: Value) -> Value {
        json!({ "tokenUsage": { "total": total, "last": last, "modelContextWindow": 200_000 } })
    }

    #[test]
    fn a_fresh_threads_first_update_subtracts_its_own_last_request_as_the_baseline() {
        let mut tokens = TurnTokens::default();
        tokens.begin();
        let first = process_notification(
            "thread/tokenUsage/updated",
            &token_usage(breakdown(100, 20, 40, 120), breakdown(100, 20, 40, 90)),
            &mut tokens,
        );
        let usage = first[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(100), Some(20), Some(40))
        );
        assert_eq!(usage.context_used_tokens, Some(90));
        assert_eq!(usage.context_limit_tokens, Some(200_000));

        let second = process_notification(
            "thread/tokenUsage/updated",
            &token_usage(breakdown(250, 50, 90, 300), breakdown(150, 30, 50, 180)),
            &mut tokens,
        );
        let usage = second[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(250), Some(50), Some(90))
        );
    }

    #[test]
    fn a_resumed_thread_counts_only_what_this_turn_used_on_top_of_prior_totals() {
        let mut tokens = TurnTokens::default();
        let replay = process_notification(
            "thread/tokenUsage/updated",
            &token_usage(breakdown(1000, 200, 500, 1200), breakdown(80, 10, 40, 90)),
            &mut tokens,
        );
        assert_eq!(replay[0].usage.as_ref().unwrap().input_tokens, None);
        tokens.begin();
        let during = process_notification(
            "thread/tokenUsage/updated",
            &token_usage(breakdown(1150, 230, 560, 1380), breakdown(150, 30, 60, 180)),
            &mut tokens,
        );
        let usage = during[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(150), Some(30), Some(60))
        );
    }

    #[test]
    fn without_a_last_request_the_turns_tokens_are_left_unreported() {
        let mut tokens = TurnTokens::default();
        tokens.begin();
        let events = process_notification(
            "thread/tokenUsage/updated",
            &json!({ "tokenUsage": { "total": breakdown(100, 20, 40, 120), "modelContextWindow": 200_000 } }),
            &mut tokens,
        );
        let usage = events[0].usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.context_limit_tokens, Some(200_000));
    }

    #[test]
    fn a_total_that_goes_backwards_is_clamped_to_zero() {
        let mut tokens = TurnTokens::default();
        let before = token_usage(breakdown(100, 20, 40, 120), breakdown(10, 2, 4, 12));
        tokens.consumed(&before["tokenUsage"]);
        tokens.begin();
        let after = token_usage(breakdown(50, 10, 20, 60), breakdown(10, 2, 4, 12));
        assert_eq!(
            tokens.consumed(&after["tokenUsage"]),
            Some(Counts {
                input: 0,
                output: 0,
                cached: 0
            })
        );
    }

    #[test]
    fn a_retryable_error_notification_is_not_surfaced() {
        assert!(
            run(
                "error",
                json!({ "error": { "message": "rate" }, "willRetry": true })
            )
            .is_empty()
        );
        let events = run("error", json!({ "error": { "message": "boom" } }));
        assert_eq!(events[0].event, kinds::ERROR);
        assert_eq!(events[0].content.as_deref(), Some("boom"));
    }

    #[test]
    fn turn_completed_ends_the_turn_and_failure_becomes_an_error() {
        let done = process_turn_completed(&json!({ "turn": { "status": "completed" } }));
        assert_eq!(done.event, kinds::TURN_COMPLETE);
        let failed = process_turn_completed(
            &json!({ "turn": { "status": "failed", "error": { "message": "nope" } } }),
        );
        assert_eq!(failed.event, kinds::ERROR);
        assert_eq!(failed.content.as_deref(), Some("nope"));
        let interrupted = process_turn_completed(&json!({ "turn": { "status": "interrupted" } }));
        assert_eq!(
            interrupted.content.as_deref(),
            Some("Codex turn was interrupted")
        );
        let bare = process_turn_completed(&json!({}));
        assert_eq!(
            bare.content.as_deref(),
            Some("Codex could not complete this turn")
        );
    }

    #[test]
    fn unknown_methods_and_non_object_params_produce_nothing() {
        assert!(run("turn/started", json!({})).is_empty());
        assert!(run("item/agentMessage/delta", json!("x")).is_empty());
    }
}
