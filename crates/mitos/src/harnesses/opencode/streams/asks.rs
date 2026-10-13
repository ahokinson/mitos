use serde_json::{Value, json};

use crate::domain::RequestKind;
use crate::json::{Record, clip, string_value};
use crate::wire::events::{AdapterEvent, kinds};

/// A server-initiated ask awaiting a host decision.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingAsk {
    pub id: String,
    pub kind: RequestKind,
    pub question_count: usize,
}

pub struct AskReply {
    pub path: String,
    pub body: Value,
}

/// The pending record for a `request` event the stream produced.
pub fn pending_for(event: &AdapterEvent) -> Option<PendingAsk> {
    let payload = event.payload.as_ref()?;
    let id = string_value(payload.get("id"))?;
    match payload.get("kind").and_then(Value::as_str) {
        Some("permission") => Some(PendingAsk {
            id: id.into(),
            kind: RequestKind::Permission,
            question_count: 0,
        }),
        Some("question") => {
            let count = payload
                .pointer("/params/questions")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            Some(PendingAsk {
                id: id.into(),
                kind: RequestKind::Question,
                question_count: count.max(1),
            })
        }
        _ => None,
    }
}

/// The HTTP reply answering `pending`, from the response shapes the TUI sends:
/// `{allow}` for permissions, `{answer}` for questions.
pub fn reply_for(pending: &PendingAsk, response: &Value) -> AskReply {
    if pending.kind == RequestKind::Question {
        let text = string_value(response.get("answer")).unwrap_or("");
        let answers: Vec<Vec<&str>> = (0..pending.question_count).map(|_| vec![text]).collect();
        return AskReply {
            path: format!("/question/{}/reply", pending.id),
            body: json!({ "answers": answers }),
        };
    }
    let allowed = response.get("allow") == Some(&Value::Bool(true));
    AskReply {
        path: format!("/permission/{}/reply", pending.id),
        body: json!({ "reply": if allowed { "once" } else { "reject" } }),
    }
}

pub(super) fn permission_asked(properties: &Record) -> Option<AdapterEvent> {
    let id = string_value(properties.get("id"))?;
    let permission = string_value(properties.get("permission")).unwrap_or("permission");
    let patterns: Vec<&str> = properties
        .get("patterns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let title = if patterns.is_empty() {
        clip(permission)
    } else {
        clip(&format!("{permission}: {}", patterns.join(", ")))
    };
    Some(request(id, RequestKind::Permission, &title, properties))
}

pub(super) fn question_asked(properties: &Record) -> Option<AdapterEvent> {
    let id = string_value(properties.get("id"))?;
    let first = properties
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(Value::as_object);
    let options: Vec<&str> = first
        .and_then(|first| first.get("options"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| string_value(option.get("label")))
        .collect();
    let text = string_value(first.and_then(|first| first.get("question"))).unwrap_or("question");
    let title = if options.is_empty() {
        clip(text)
    } else {
        clip(&format!("{text} [{}]", options.join(" / ")))
    };
    Some(request(id, RequestKind::Question, &title, properties))
}

fn request(id: &str, kind: RequestKind, title: &str, params: &Record) -> AdapterEvent {
    AdapterEvent {
        payload: Some(json!({
            "id": id,
            "kind": kind.as_str(),
            "title": title,
            "params": params,
        })),
        ..AdapterEvent::new(kinds::REQUEST)
    }
}
