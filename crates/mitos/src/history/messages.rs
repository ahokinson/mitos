use serde_json::Value;

use crate::handoff::PREAMBLE_PREFIX;
use crate::json::{
    Record, field, find_string, first_present, object_field, string_value, text_value,
};
use crate::wire::responses::CollectedMessage;

pub const USER: &str = "user";
pub const ASSISTANT: &str = "assistant";

/// The prompt Mitos sends a harness to catch it up; reading it back from that
/// harness's history would record it as something the user said.
pub fn is_handoff_preamble(message: &CollectedMessage) -> bool {
    message.role == USER && message.text.starts_with(PREAMBLE_PREFIX)
}

/// Claude logs a tool's output as a `user` record of `tool_result` blocks.
fn has_tool_result(content: &Value) -> bool {
    content.as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            block.as_object().is_some_and(|block| {
                block.get("type").and_then(Value::as_str) == Some("tool_result")
            })
        })
    })
}

pub fn messages_from_record(record: &Value) -> Vec<CollectedMessage> {
    let Some(record) = record.as_object() else {
        return Vec::new();
    };
    let payload = payload_of(record);
    let message = object_field(Some(payload), "message");
    let role =
        string_value(field(Some(payload), "role")).or_else(|| string_value(field(message, "role")));
    let Some(role) = role.filter(|role| *role == USER || *role == ASSISTANT) else {
        return Vec::new();
    };
    let Some(content) = first_present(&[
        field(Some(payload), "content"),
        field(message, "content"),
        field(Some(payload), "text"),
        field(message, "text"),
    ]) else {
        return Vec::new();
    };
    if role == USER && has_tool_result(content) {
        return Vec::new();
    }
    let Some(text) = text_value(content) else {
        return Vec::new();
    };
    let mined = CollectedMessage {
        role: role.into(),
        text,
    };
    if is_handoff_preamble(&mined) {
        Vec::new()
    } else {
        vec![mined]
    }
}

/// Codex wraps its records in `payload`; Claude's are the record itself.
pub fn payload_of(record: &Record) -> &Record {
    object_field(Some(record), "payload").unwrap_or(record)
}

pub fn matches_workspace(records: &[Value], workdir: &str) -> bool {
    records.iter().any(|record| {
        find_string(record, &["cwd", "workdir", "workspace"]).is_some_and(|found| found == workdir)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn mines_claude_and_codex_shaped_records() {
        let claude = json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": "hello" }] } });
        let codex = json!({ "payload": { "role": "user", "content": [{ "type": "input_text", "text": "hi" }] } });
        assert_eq!(messages_from_record(&claude)[0].text, "hello");
        let mined = &messages_from_record(&codex)[0];
        assert_eq!((mined.role.as_str(), mined.text.as_str()), ("user", "hi"));
    }

    #[test]
    fn skips_tool_results_preambles_and_other_roles() {
        let tool = json!({ "message": { "role": "user", "content": [{ "type": "tool_result", "content": "out" }] } });
        let preamble = json!({ "role": "user", "content": format!("{PREAMBLE_PREFIX}t1. rest") });
        let system = json!({ "role": "system", "content": "x" });
        assert!(messages_from_record(&tool).is_empty());
        assert!(messages_from_record(&preamble).is_empty());
        assert!(messages_from_record(&system).is_empty());
        assert!(messages_from_record(&json!("text")).is_empty());
    }

    #[test]
    fn workspace_matches_any_cwd_like_key() {
        let records = vec![json!({ "a": { "cwd": "/work" } })];
        assert!(matches_workspace(&records, "/work"));
        assert!(!matches_workspace(&records, "/other"));
    }
}
