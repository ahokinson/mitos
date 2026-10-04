use std::collections::HashSet;

use serde_json::{Value, json};

use crate::domain::RequestKind;
use crate::json::{Record, clip, count_value, number_value, string_value};
use crate::rpc::RpcId;
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::responses::CollectedUsage;

pub const SESSION_UPDATE: &str = "session/update";
pub const REQUEST_PERMISSION: &str = "session/request_permission";

const ASSISTANT: &str = "assistant";
const TOOL: &str = "tool";
const END_TURN: &str = "end_turn";

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

/// Maps one ACP session's `session/update` notifications onto Mitos adapter
/// events. ACP streams assistant text only as chunks, so the text is also
/// emitted whole as one message before each tool call and at the end of the
/// turn. Reasoning chunks are intentionally not surfaced.
pub struct AcpStream {
    session_id: String,
    text: String,
    called: HashSet<String>,
    resolved: HashSet<String>,
}

impl AcpStream {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.into(),
            text: String::new(),
            called: HashSet::new(),
            resolved: HashSet::new(),
        }
    }

    /// Updates for other sessions yield nothing.
    pub fn update(&mut self, params: &Value) -> Vec<AdapterEvent> {
        if string_value(params.get("sessionId")) != Some(self.session_id.as_str()) {
            return Vec::new();
        }
        let Some(update) = params.get("update").and_then(Value::as_object) else {
            return Vec::new();
        };
        match string_value(update.get("sessionUpdate")) {
            Some("agent_message_chunk") => self.message_chunk(update),
            Some("tool_call") => self.tool_call(update),
            Some("tool_call_update") => self.tool_call_update(update),
            Some("usage_update") => usage_update(update),
            _ => Vec::new(),
        }
    }

    /// The terminal events for a finished `session/prompt`.
    pub fn finish(&mut self, result: &Value) -> Vec<AdapterEvent> {
        let mut events = self.flush();
        if let Some(usage) = usage_from_result(result.get("usage")) {
            events.push(AdapterEvent {
                usage: Some(usage),
                ..AdapterEvent::new(kinds::USAGE)
            });
        }
        let stop = string_value(result.get("stopReason")).unwrap_or(END_TURN);
        if stop == END_TURN {
            events.push(AdapterEvent::new(kinds::TURN_COMPLETE));
        } else {
            events.push(event(
                kinds::ERROR,
                None,
                Some(stop_message(stop)),
                result.is_object().then(|| result.clone()),
            ));
        }
        events
    }

    fn message_chunk(&mut self, update: &Record) -> Vec<AdapterEvent> {
        let content = update.get("content").and_then(Value::as_object);
        let text = content
            .filter(|content| content.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|content| string_value(content.get("text")));
        let Some(text) = text else {
            return Vec::new();
        };
        self.text.push_str(text);
        vec![event(
            kinds::ASSISTANT_DELTA,
            Some(ASSISTANT),
            Some(text.into()),
            None,
        )]
    }

    fn tool_call(&mut self, update: &Record) -> Vec<AdapterEvent> {
        let Some(id) = string_value(update.get("toolCallId")) else {
            return Vec::new();
        };
        if !self.called.insert(id.into()) {
            return Vec::new();
        }
        let mut events = self.flush();
        events.push(event(
            kinds::TOOL_CALL,
            None,
            Some(clip(string_value(update.get("title")).unwrap_or("tool"))),
            Some(Value::Object(update.clone())),
        ));
        events
    }

    fn tool_call_update(&mut self, update: &Record) -> Vec<AdapterEvent> {
        let Some(id) = string_value(update.get("toolCallId")) else {
            return Vec::new();
        };
        let status = string_value(update.get("status"));
        if self.resolved.contains(id) || !matches!(status, Some("completed" | "failed")) {
            return Vec::new();
        }
        self.resolved.insert(id.into());
        let content = tool_output(update)
            .or_else(|| string_value(update.get("title")).map(str::to_string))
            .or_else(|| status.map(str::to_string));
        vec![event(
            kinds::TOOL_RESULT,
            Some(TOOL),
            content,
            Some(Value::Object(update.clone())),
        )]
    }

    fn flush(&mut self) -> Vec<AdapterEvent> {
        let text = std::mem::take(&mut self.text);
        let text = text.trim();
        if text.is_empty() {
            return Vec::new();
        }
        vec![event(
            kinds::ASSISTANT_MESSAGE,
            Some(ASSISTANT),
            Some(text.into()),
            None,
        )]
    }
}

fn usage_update(update: &Record) -> Vec<AdapterEvent> {
    let used = count_value(update.get("used"));
    let size = count_value(update.get("size"));
    let cost = update.get("cost").and_then(Value::as_object);
    let cost_usd = cost
        .filter(|cost| string_value(cost.get("currency")) == Some("USD"))
        .and_then(|cost| number_value(cost.get("amount")));
    if used.is_none() && size.is_none() && cost_usd.is_none() {
        return Vec::new();
    }
    vec![AdapterEvent {
        usage: Some(CollectedUsage {
            context_used_tokens: used,
            context_limit_tokens: size,
            cost_usd,
            ..CollectedUsage::default()
        }),
        ..AdapterEvent::new(kinds::USAGE)
    }]
}

fn tool_output(update: &Record) -> Option<String> {
    let text = update
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            let inner = block.get("content")?.as_object()?;
            if inner.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            string_value(inner.get("text"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        return Some(text);
    }
    match update.get("rawOutput") {
        None => None,
        Some(Value::String(raw)) if !raw.is_empty() => Some(raw.clone()),
        Some(raw) => Some(clip(&raw.to_string())),
    }
}

fn usage_from_result(value: Option<&Value>) -> Option<CollectedUsage> {
    let usage = value?.as_object()?;
    let input = count_value(usage.get("inputTokens"));
    let output = count_value(usage.get("outputTokens"));
    let cached = count_value(usage.get("cachedReadTokens"));
    if input.is_none() && output.is_none() && cached.is_none() {
        return None;
    }
    Some(CollectedUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        ..CollectedUsage::default()
    })
}

fn stop_message(stop: &str) -> String {
    match stop {
        "cancelled" => "Hermes turn was cancelled".into(),
        "refusal" => "Hermes refused this turn".into(),
        "max_tokens" => "Hermes stopped at its token limit".into(),
        "max_turn_requests" => "Hermes stopped at its request limit for the turn".into(),
        other => format!("Hermes stopped: {other}"),
    }
}

/// A permission request awaiting a host decision.
#[derive(Clone, Debug)]
pub struct PendingPermission {
    pub rpc_id: RpcId,
    options: Vec<PermissionOption>,
}

#[derive(Clone, Debug)]
struct PermissionOption {
    option_id: String,
    kind: String,
}

/// `None` when the request has nothing the host could decide.
pub fn permission_request(
    rpc_id: &RpcId,
    params: &Value,
) -> Option<(AdapterEvent, PendingPermission)> {
    let options: Vec<PermissionOption> = params
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|option| {
            Some(PermissionOption {
                option_id: string_value(option.get("optionId"))?.into(),
                kind: string_value(option.get("kind")).unwrap_or("").into(),
            })
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    let title = clip(
        string_value(params.get("toolCall").and_then(|call| call.get("title")))
            .unwrap_or("Permission requested"),
    );
    let payload = json!({
        "id": rpc_id.to_text(),
        "kind": RequestKind::Permission.as_str(),
        "title": title,
        "method": REQUEST_PERMISSION,
        "params": if params.is_object() { params.clone() } else { json!({}) },
    });
    let event = AdapterEvent {
        payload: Some(payload),
        ..AdapterEvent::new(kinds::REQUEST)
    };
    let pending = PendingPermission {
        rpc_id: rpc_id.clone(),
        options,
    };
    Some((event, pending))
}

/// The `session/request_permission` result for the TUI's `{allow}` answer.
pub fn outcome_for(pending: &PendingPermission, response: &Value) -> Value {
    let allow = response.get("allow") == Some(&Value::Bool(true));
    let preferred = if allow {
        ["allow_once", "allow_always"]
    } else {
        ["reject_once", "reject_always"]
    };
    preferred
        .iter()
        .find_map(|kind| pending.options.iter().find(|option| option.kind == *kind))
        .map_or_else(
            || json!({ "outcome": { "outcome": "cancelled" } }),
            |option| json!({ "outcome": { "outcome": "selected", "optionId": option.option_id } }),
        )
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use super::*;
    use crate::harnesses::fixtures::kinds_of;

    const SESSION: &str = "sess-1";

    fn update(body: Value) -> Value {
        json!({ "sessionId": SESSION, "update": body })
    }

    fn chunk(text: &str) -> Value {
        update(
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }),
        )
    }

    #[test]
    fn message_chunks_stream_as_deltas() {
        let events = AcpStream::new(SESSION).update(&chunk("Hel"));
        assert_eq!(kinds_of(&events), [kinds::ASSISTANT_DELTA]);
        assert_eq!(events[0].role.as_deref(), Some("assistant"));
        assert_eq!(events[0].content.as_deref(), Some("Hel"));
    }

    #[test]
    fn other_sessions_reasoning_and_non_text_content_are_ignored() {
        let mut stream = AcpStream::new(SESSION);
        let other = json!({ "sessionId": "other", "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "x" } } });
        assert!(stream.update(&other).is_empty());
        let thought = update(
            json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "hmm" } }),
        );
        assert!(stream.update(&thought).is_empty());
        let image = update(
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "image", "data": "..." } }),
        );
        assert!(stream.update(&image).is_empty());
    }

    #[test]
    fn the_turn_ends_with_the_whole_text_as_one_message_then_turn_complete() {
        let mut stream = AcpStream::new(SESSION);
        stream.update(&chunk("Hel"));
        stream.update(&chunk("lo"));
        let events = stream.finish(&json!({ "stopReason": "end_turn" }));
        assert_eq!(
            kinds_of(&events),
            [kinds::ASSISTANT_MESSAGE, kinds::TURN_COMPLETE]
        );
        assert_eq!(events[0].content.as_deref(), Some("Hello"));
    }

    #[test]
    fn text_before_a_tool_call_is_flushed_as_a_message_first() {
        let mut stream = AcpStream::new(SESSION);
        stream.update(&chunk("Let me look."));
        let events = stream.update(&update(
            json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Read a.ts" }),
        ));
        assert_eq!(
            kinds_of(&events),
            [kinds::ASSISTANT_MESSAGE, kinds::TOOL_CALL]
        );
        assert_eq!(events[0].content.as_deref(), Some("Let me look."));
        assert_eq!(events[1].content.as_deref(), Some("Read a.ts"));
        let end = stream.finish(&json!({ "stopReason": "end_turn" }));
        assert_eq!(kinds_of(&end), [kinds::TURN_COMPLETE]);
    }

    #[test]
    fn a_tool_call_yields_one_call_and_one_result_with_its_text_output() {
        let mut stream = AcpStream::new(SESSION);
        let call =
            update(json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "ls" }));
        assert_eq!(stream.update(&call).len(), 1);
        assert!(stream.update(&call).is_empty());
        let progress = update(
            json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "in_progress" }),
        );
        assert!(stream.update(&progress).is_empty());
        let done = update(json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": "t1",
            "status": "completed",
            "content": [{ "type": "content", "content": { "type": "text", "text": "a\nb" } }]
        }));
        let result = stream.update(&done);
        assert_eq!(kinds_of(&result), [kinds::TOOL_RESULT]);
        assert_eq!(result[0].content.as_deref(), Some("a\nb"));
        assert_eq!(result[0].role.as_deref(), Some("tool"));
        assert!(stream.update(&done).is_empty());
    }

    #[test]
    fn a_failed_tool_falls_back_to_raw_output_then_title() {
        let mut stream = AcpStream::new(SESSION);
        let raw = stream.update(&update(json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "failed", "rawOutput": "permission denied" })));
        assert_eq!(raw[0].content.as_deref(), Some("permission denied"));
        let titled = stream.update(&update(json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t2", "status": "failed", "title": "Edit a.ts" })));
        assert_eq!(titled[0].content.as_deref(), Some("Edit a.ts"));
        let bare = stream.update(&update(
            json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t3", "status": "failed" }),
        ));
        assert_eq!(bare[0].content.as_deref(), Some("failed"));
    }

    #[test]
    fn usage_update_becomes_context_usage_and_carries_only_usd_cost() {
        let usage = AcpStream::new(SESSION).update(&update(
            json!({ "sessionUpdate": "usage_update", "used": 1200, "size": 128_000 }),
        ));
        let fields = usage[0].usage.as_ref().unwrap();
        assert_eq!(
            (fields.context_used_tokens, fields.context_limit_tokens),
            (Some(1200), Some(128_000))
        );
        assert_eq!(fields.cost_usd, None);

        let usd = AcpStream::new(SESSION).update(&update(json!({ "sessionUpdate": "usage_update", "used": 10, "size": 100, "cost": { "amount": 0.0123, "currency": "USD" } })));
        assert_eq!(usd[0].usage.as_ref().unwrap().cost_usd, Some(0.0123));
        let eur = AcpStream::new(SESSION).update(&update(json!({ "sessionUpdate": "usage_update", "used": 10, "size": 100, "cost": { "amount": 0.5, "currency": "EUR" } })));
        assert_eq!(eur[0].usage.as_ref().unwrap().cost_usd, None);
        let only_cost = AcpStream::new(SESSION).update(&update(
            json!({ "sessionUpdate": "usage_update", "cost": { "amount": 1, "currency": "USD" } }),
        ));
        assert_eq!(only_cost[0].usage.as_ref().unwrap().cost_usd, Some(1.0));
        let nothing =
            AcpStream::new(SESSION).update(&update(json!({ "sessionUpdate": "usage_update" })));
        assert!(nothing.is_empty());
    }

    #[test]
    fn a_prompt_results_token_usage_is_emitted_before_completion() {
        let events = AcpStream::new(SESSION).finish(&json!({
            "stopReason": "end_turn",
            "usage": { "inputTokens": 10, "outputTokens": 4, "cachedReadTokens": 2 }
        }));
        assert_eq!(kinds_of(&events), [kinds::USAGE, kinds::TURN_COMPLETE]);
        let usage = events[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(10), Some(4), Some(2))
        );
    }

    #[test]
    fn a_non_end_turn_stop_reason_is_an_error() {
        let cancelled = AcpStream::new(SESSION).finish(&json!({ "stopReason": "cancelled" }));
        assert_eq!(cancelled.last().unwrap().event, kinds::ERROR);
        assert_eq!(
            cancelled.last().unwrap().content.as_deref(),
            Some("Hermes turn was cancelled")
        );
        let refused = AcpStream::new(SESSION).finish(&json!({ "stopReason": "refusal" }));
        assert_eq!(
            refused.last().unwrap().content.as_deref(),
            Some("Hermes refused this turn")
        );
        let other = AcpStream::new(SESSION).finish(&json!({ "stopReason": "weird" }));
        assert_eq!(
            other.last().unwrap().content.as_deref(),
            Some("Hermes stopped: weird")
        );
        let empty = AcpStream::new(SESSION).finish(&Value::Null);
        assert_eq!(kinds_of(&empty), [kinds::TURN_COMPLETE]);
    }

    fn options() -> Value {
        json!([
            { "optionId": "ao", "name": "Allow once", "kind": "allow_once" },
            { "optionId": "aa", "name": "Always", "kind": "allow_always" },
            { "optionId": "ro", "name": "Reject", "kind": "reject_once" }
        ])
    }

    #[test]
    fn a_permission_request_becomes_a_request_and_answers_with_an_option_id() {
        let (event, pending) = permission_request(
            &RpcId::Number(7),
            &json!({ "toolCall": { "toolCallId": "t1", "title": "Run rm -rf build" }, "options": options() }),
        )
        .unwrap();
        assert_eq!(event.event, kinds::REQUEST);
        let payload = event.payload.unwrap();
        assert_eq!(payload["id"], "7");
        assert_eq!(payload["kind"], "permission");
        assert_eq!(payload["title"], "Run rm -rf build");
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": true })),
            json!({ "outcome": { "outcome": "selected", "optionId": "ao" } })
        );
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": false })),
            json!({ "outcome": { "outcome": "selected", "optionId": "ro" } })
        );
    }

    #[test]
    fn allow_falls_back_to_allow_always_and_a_missing_reject_cancels() {
        let (_, pending) = permission_request(
            &RpcId::Number(1),
            &json!({ "options": [{ "optionId": "aa", "kind": "allow_always" }] }),
        )
        .unwrap();
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": true })),
            json!({ "outcome": { "outcome": "selected", "optionId": "aa" } })
        );
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": false })),
            json!({ "outcome": { "outcome": "cancelled" } })
        );
    }

    #[test]
    fn a_permission_request_with_no_options_cannot_be_answered() {
        assert!(permission_request(&RpcId::Number(1), &json!({ "options": [] })).is_none());
    }
}
