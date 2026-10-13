mod blocks;

use serde_json::{Value, json};

use crate::json::{is_truthy, string_value};
use crate::wire::events::{AdapterEvent, kinds};

use blocks::{assistant_events, result_events, user_events};

/// Translates one line of `claude -p --output-format stream-json --verbose`
/// output into zero or more events. Pure, so it is testable without the CLI.
pub fn process_stream_line(record: &Value) -> Vec<AdapterEvent> {
    let Some(record) = record.as_object() else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(session) = string_value(record.get("session_id")) {
        events.push(AdapterEvent {
            native_session: Some(Value::String(session.into())),
            ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
        });
    }
    if is_truthy(record.get("parent_tool_use_id")) {
        return events;
    }
    match record.get("type").and_then(Value::as_str) {
        Some("assistant") => assistant_events(record, &mut events),
        Some("user") => user_events(record, &mut events),
        Some("result") => result_events(record, &mut events),
        _ => {}
    }
    events
}

pub fn user_message(text: &str) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": text } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_of(events: &[AdapterEvent]) -> Vec<&str> {
        events.iter().map(|event| event.event.as_str()).collect()
    }

    #[test]
    fn system_init_carries_the_session_id() {
        let events = process_stream_line(
            &json!({ "type": "system", "subtype": "init", "session_id": "abc-123", "model": "m" }),
        );
        assert_eq!(kinds_of(&events), [kinds::NATIVE_SESSION_UPDATE]);
        assert_eq!(events[0].native_session, Some(json!("abc-123")));
    }

    #[test]
    fn assistant_text_tool_use_and_usage_become_events() {
        let events = process_stream_line(&json!({
            "type": "assistant",
            "session_id": "abc-123",
            "message": {
                "content": [
                    { "type": "text", "text": "Let me check that file." },
                    { "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "a.ts" } }
                ],
                "usage": { "input_tokens": 10, "output_tokens": 5 },
                "model": "claude-sonnet-5"
            }
        }));
        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_MESSAGE,
                kinds::TOOL_CALL,
                kinds::USAGE
            ]
        );
        assert_eq!(events[1].role.as_deref(), Some("assistant"));
        assert_eq!(
            events[1].content.as_deref(),
            Some("Let me check that file.")
        );
        assert_eq!(events[2].content.as_deref(), Some("Read"));
        assert_eq!(
            events[2].payload,
            Some(
                json!({ "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "a.ts" } })
            )
        );
        let usage = events[3].usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.cached_input_tokens, None);
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-5"));
    }

    #[test]
    fn user_tool_result_becomes_a_tool_result_event() {
        let events = process_stream_line(&json!({
            "type": "user",
            "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "file contents here" }] }
        }));
        assert_eq!(kinds_of(&events), [kinds::TOOL_RESULT]);
        assert_eq!(events[0].role.as_deref(), Some("tool"));
        assert_eq!(events[0].content.as_deref(), Some("file contents here"));
        assert_eq!(
            events[0].payload,
            Some(
                json!({ "type": "tool_result", "tool_use_id": "t1", "content": "file contents here" })
            )
        );
    }

    #[test]
    fn tool_use_result_is_added_to_the_payload_when_present() {
        let events = process_stream_line(&json!({
            "type": "user",
            "tool_use_result": { "ok": true },
            "message": { "content": [{ "type": "tool_result", "content": [{ "text": "a" }, { "text": "b" }] }] }
        }));
        assert_eq!(events[0].content.as_deref(), Some("a\nb"));
        assert_eq!(
            events[0].payload.as_ref().unwrap()["tool_use_result"],
            json!({ "ok": true })
        );
    }

    #[test]
    fn subagent_messages_are_skipped() {
        let events = process_stream_line(&json!({
            "type": "assistant",
            "parent_tool_use_id": "t1",
            "message": { "content": [{ "type": "text", "text": "subagent chatter" }] }
        }));
        assert!(events.is_empty());
    }

    #[test]
    fn result_emits_usage_then_turn_complete() {
        let events = process_stream_line(&json!({
            "type": "result",
            "usage": { "input_tokens": 100, "output_tokens": 40 }
        }));
        assert_eq!(kinds_of(&events), [kinds::USAGE, kinds::TURN_COMPLETE]);
        let usage = events[0].usage.as_ref().unwrap();
        assert_eq!(
            (usage.input_tokens, usage.output_tokens),
            (Some(100), Some(40))
        );
        assert_eq!(usage.cost_usd, None);
    }

    #[test]
    fn result_carries_total_cost_with_or_without_a_usage_object() {
        let with_usage = process_stream_line(&json!({
            "type": "result",
            "total_cost_usd": 0.0421,
            "usage": { "input_tokens": 100, "output_tokens": 40 }
        }));
        let usage = with_usage[0].usage.as_ref().unwrap();
        assert_eq!(usage.cost_usd, Some(0.0421));
        assert_eq!(usage.input_tokens, Some(100));

        let cost_only = process_stream_line(&json!({ "type": "result", "total_cost_usd": 0.5 }));
        assert_eq!(kinds_of(&cost_only), [kinds::USAGE, kinds::TURN_COMPLETE]);
        assert_eq!(cost_only[0].usage.as_ref().unwrap().cost_usd, Some(0.5));
    }

    #[test]
    fn non_objects_and_unknown_lines_produce_nothing() {
        assert!(process_stream_line(&json!("not an object")).is_empty());
        assert!(process_stream_line(&json!({ "type": "stream_event" })).is_empty());
    }
}
