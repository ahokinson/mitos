use serde_json::{Map, Value, json};

use crate::domain::RequestKind;
use crate::json::{Record, clip, string_value};
use crate::rpc::RpcId;
use crate::wire::events::{AdapterEvent, kinds};

const COMMAND_APPROVAL: &str = "item/commandExecution/requestApproval";
const FILE_APPROVAL: &str = "item/fileChange/requestApproval";
const USER_INPUT: &str = "item/tool/requestUserInput";

/// A server-initiated request awaiting a host decision.
#[derive(Clone, Debug)]
pub struct PendingServerRequest {
    pub rpc_id: RpcId,
    pub kind: RequestKind,
    pub question_ids: Vec<String>,
}

/// `None` for server requests Mitos can't answer; the caller rejects those.
pub fn request_from_server(
    rpc_id: &RpcId,
    method: &str,
    params: &Value,
) -> Option<(AdapterEvent, PendingServerRequest)> {
    let empty = Map::new();
    let body = params.as_object().unwrap_or(&empty);
    let (kind, title, question_ids) = match method {
        COMMAND_APPROVAL | FILE_APPROVAL => (
            RequestKind::Permission,
            approval_title(method, body),
            Vec::new(),
        ),
        USER_INPUT => question_summary(body),
        _ => return None,
    };
    let payload = json!({
        "id": rpc_id.to_text(),
        "kind": kind.as_str(),
        "title": title,
        "method": method,
        "params": body,
    });
    let event = AdapterEvent {
        payload: Some(payload),
        ..AdapterEvent::new(kinds::REQUEST)
    };
    let pending = PendingServerRequest {
        rpc_id: rpc_id.clone(),
        kind,
        question_ids,
    };
    Some((event, pending))
}

fn approval_title(method: &str, body: &Record) -> String {
    let subject = if method == COMMAND_APPROVAL {
        format!(
            "Run: {}",
            string_value(body.get("command")).unwrap_or("command")
        )
    } else {
        "Apply file changes".into()
    };
    match string_value(body.get("reason")) {
        Some(reason) => clip(&format!("{subject} ({reason})")),
        None => clip(&subject),
    }
}

fn question_summary(body: &Record) -> (RequestKind, String, Vec<String>) {
    let questions: Vec<&Record> = body
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    let ids = questions
        .iter()
        .map(|question| string_value(question.get("id")).unwrap_or("").to_string())
        .collect();
    let first = questions.first();
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
    (RequestKind::Question, title, ids)
}

/// The JSON-RPC `result` answering `pending`, from the response shapes the
/// TUI sends: `{allow}` for permissions, `{answer}` for questions.
pub fn result_for(pending: &PendingServerRequest, response: &Value) -> Value {
    let answer = response.get("answer");
    if pending.kind == RequestKind::Question {
        let text = string_value(answer).unwrap_or("");
        let answers: Map<String, Value> = pending
            .question_ids
            .iter()
            .map(|id| (id.clone(), json!({ "answers": [text] })))
            .collect();
        return json!({ "answers": answers });
    }
    let allowed = response.get("allow") == Some(&Value::Bool(true));
    json!({ "decision": if allowed { "accept" } else { "decline" } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_approval_becomes_a_permission_request_answered_with_accept_or_decline() {
        let (event, pending) = request_from_server(
            &RpcId::Number(7),
            COMMAND_APPROVAL,
            &json!({ "command": "make clean", "reason": "outside sandbox" }),
        )
        .unwrap();
        let payload = event.payload.unwrap();
        assert_eq!(payload["id"], "7");
        assert_eq!(payload["kind"], "permission");
        assert_eq!(payload["title"], "Run: make clean (outside sandbox)");
        assert_eq!(
            result_for(&pending, &json!({ "allow": true })),
            json!({ "decision": "accept" })
        );
        assert_eq!(
            result_for(&pending, &json!({ "allow": false })),
            json!({ "decision": "decline" })
        );
        assert_eq!(
            result_for(&pending, &json!("not an object")),
            json!({ "decision": "decline" })
        );
    }

    #[test]
    fn file_approval_has_its_own_title() {
        let (event, _) =
            request_from_server(&RpcId::Text("f".into()), FILE_APPROVAL, &json!({})).unwrap();
        assert_eq!(event.payload.unwrap()["title"], "Apply file changes");
    }

    #[test]
    fn user_input_requests_become_questions_answered_per_question_id() {
        let (event, pending) = request_from_server(
            &RpcId::Text("q-1".into()),
            USER_INPUT,
            &json!({ "questions": [{
                "id": "db",
                "question": "Which db?",
                "options": [{ "label": "pg" }, { "label": "sqlite" }]
            }] }),
        )
        .unwrap();
        assert_eq!(pending.kind, RequestKind::Question);
        assert_eq!(event.payload.unwrap()["title"], "Which db? [pg / sqlite]");
        assert_eq!(
            result_for(&pending, &json!({ "answer": "pg" })),
            json!({ "answers": { "db": { "answers": ["pg"] } } })
        );
    }

    #[test]
    fn server_requests_mitos_cannot_answer_are_not_turned_into_requests() {
        assert!(
            request_from_server(&RpcId::Number(1), "attestation/generate", &json!({})).is_none()
        );
    }
}
