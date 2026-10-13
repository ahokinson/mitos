use serde_json::{Map, Value};

use crate::json::{Record, count_value, number_value};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::responses::CollectedUsage;

const ASSISTANT: &str = "assistant";
const TOOL: &str = "tool";

fn blocks(message: &Record) -> impl Iterator<Item = &Record> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

pub(super) fn assistant_events(record: &Record, events: &mut Vec<AdapterEvent>) {
    let empty = Map::new();
    let message = record
        .get("message")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    for block in blocks(message) {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    events.push(AdapterEvent {
                        role: Some(ASSISTANT.into()),
                        content: Some(text.into()),
                        ..AdapterEvent::new(kinds::ASSISTANT_MESSAGE)
                    });
                }
            }
            Some("tool_use") => {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    events.push(AdapterEvent {
                        role: Some(ASSISTANT.into()),
                        content: Some(name.into()),
                        payload: Some(Value::Object(block.clone())),
                        ..AdapterEvent::new(kinds::TOOL_CALL)
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(usage) = message.get("usage").and_then(Value::as_object) {
        events.push(AdapterEvent {
            usage: Some(usage_from_claude_usage(usage, message.get("model"))),
            ..AdapterEvent::new(kinds::USAGE)
        });
    }
}

pub(super) fn user_events(record: &Record, events: &mut Vec<AdapterEvent>) {
    let empty = Map::new();
    let message = record
        .get("message")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    for block in blocks(message) {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let mut payload = block.clone();
        if let Some(result) = record.get("tool_use_result") {
            payload.insert("tool_use_result".into(), result.clone());
        }
        events.push(AdapterEvent {
            role: Some(TOOL.into()),
            content: Some(text_from_tool_result(block.get("content"))),
            payload: Some(Value::Object(payload)),
            ..AdapterEvent::new(kinds::TOOL_RESULT)
        });
    }
}

pub(super) fn result_events(record: &Record, events: &mut Vec<AdapterEvent>) {
    let cost = number_value(record.get("total_cost_usd"));
    let usage = record.get("usage").and_then(Value::as_object);
    if usage.is_some() || cost.is_some() {
        let empty = Map::new();
        events.push(AdapterEvent {
            usage: Some(CollectedUsage {
                cost_usd: cost,
                ..usage_from_claude_usage(usage.unwrap_or(&empty), None)
            }),
            ..AdapterEvent::new(kinds::USAGE)
        });
    }
    events.push(AdapterEvent::new(kinds::TURN_COMPLETE));
}

fn text_from_tool_result(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn usage_from_claude_usage(usage: &Record, model: Option<&Value>) -> CollectedUsage {
    CollectedUsage {
        input_tokens: count_value(usage.get("input_tokens")),
        output_tokens: count_value(usage.get("output_tokens")),
        cached_input_tokens: count_value(usage.get("cache_read_input_tokens"))
            .or_else(|| count_value(usage.get("cache_creation_input_tokens"))),
        model: model.and_then(Value::as_str).map(str::to_string),
        ..CollectedUsage::default()
    }
}
