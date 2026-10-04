use serde_json::{Value, json};

use crate::domain::RequestKind;
use crate::json::{Record, clip};
use crate::wire::events::{AdapterEvent, kinds};

/// A `can_use_tool` control request awaiting a host decision.
#[derive(Clone, Debug)]
pub struct PendingControl {
    pub cli_request_id: String,
    pub kind: RequestKind,
    pub input: Record,
}

fn questions_of(input: &Record) -> Vec<&Record> {
    input
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect()
}

fn first_question_text(input: &Record) -> String {
    questions_of(input)
        .first()
        .and_then(|first| first.get("question"))
        .and_then(Value::as_str)
        .unwrap_or("question")
        .to_string()
}

fn title_for(kind: RequestKind, tool_name: &str, input: &Record) -> String {
    match kind {
        RequestKind::PlanApproval => {
            clip(input.get("plan").and_then(Value::as_str).unwrap_or("plan"))
        }
        RequestKind::Question => {
            let text = first_question_text(input);
            let options: Vec<&str> = questions_of(input)
                .first()
                .and_then(|first| first.get("options"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|option| option.get("label").and_then(Value::as_str))
                .filter(|label| !label.is_empty())
                .collect();
            if options.is_empty() {
                clip(&text)
            } else {
                clip(&format!("{text} [{}]", options.join(" / ")))
            }
        }
        RequestKind::Permission => {
            let detail = match input.get("command").and_then(Value::as_str) {
                Some(command) => command.to_string(),
                None => Value::Object(input.clone()).to_string(),
            };
            clip(&format!("{tool_name} {detail}"))
        }
    }
}

/// Translates a `control_request` / `can_use_tool` line into a `request`
/// event, plus what's needed to answer it later. `None` for other traffic.
pub fn request_from_control(record: &Value) -> Option<(AdapterEvent, PendingControl)> {
    let record = record.as_object()?;
    if record.get("type")?.as_str()? != "control_request" {
        return None;
    }
    let request = record.get("request")?.as_object()?;
    if request.get("subtype")?.as_str()? != "can_use_tool" {
        return None;
    }
    let request_id = record.get("request_id")?.as_str()?;
    let tool_name = request.get("tool_name")?.as_str()?;
    let input = request
        .get("input")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let kind = match tool_name {
        "ExitPlanMode" => RequestKind::PlanApproval,
        "AskUserQuestion" => RequestKind::Question,
        _ => RequestKind::Permission,
    };
    let payload = json!({
        "id": request_id,
        "kind": kind.as_str(),
        "title": title_for(kind, tool_name, &input),
        "tool_name": tool_name,
        "input": input,
    });
    let event = AdapterEvent {
        payload: Some(payload),
        ..AdapterEvent::new(kinds::REQUEST)
    };
    let pending = PendingControl {
        cli_request_id: request_id.into(),
        kind,
        input,
    };
    Some((event, pending))
}

fn text_of(value: Option<&Value>) -> Option<String> {
    let trimmed = value?.as_str()?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The `control_response` line answering `pending`, from the response shapes
/// the TUI sends: `{allow, message?}`, `{approved, feedback?}`, `{answer}`.
pub fn control_response_for(pending: &PendingControl, response: &Value) -> Value {
    let empty = Record::new();
    let answer = response.as_object().unwrap_or(&empty);
    let allow = |input: &Record| json!({ "behavior": "allow", "updatedInput": input });
    let deny = |message: String| json!({ "behavior": "deny", "message": message });
    let body = match pending.kind {
        RequestKind::Permission => {
            if answer.get("allow") == Some(&Value::Bool(true)) {
                allow(&pending.input)
            } else {
                deny(text_of(answer.get("message")).unwrap_or_else(|| "Denied by the user.".into()))
            }
        }
        RequestKind::PlanApproval => {
            if answer.get("approved") == Some(&Value::Bool(true)) {
                allow(&pending.input)
            } else {
                deny(
                    text_of(answer.get("feedback"))
                        .unwrap_or_else(|| "The user rejected the plan.".into()),
                )
            }
        }
        RequestKind::Question => {
            let mut updated = pending.input.clone();
            let reply = text_of(answer.get("answer")).unwrap_or_default();
            updated.insert(
                "answers".into(),
                json!({ first_question_text(&pending.input): reply }),
            );
            json!({ "behavior": "allow", "updatedInput": updated })
        }
    };
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": pending.cli_request_id,
            "response": body,
        },
    })
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use super::*;

    fn bash_request() -> Value {
        json!({
            "type": "control_request",
            "request_id": "req-1",
            "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": { "command": "make clean" } }
        })
    }

    fn control(tool: &str, input: Value) -> Value {
        json!({
            "type": "control_request",
            "request_id": "req-1",
            "request": { "subtype": "can_use_tool", "tool_name": tool, "input": input }
        })
    }

    #[test]
    fn can_use_tool_becomes_a_permission_request_event() {
        let (event, pending) = request_from_control(&bash_request()).unwrap();
        assert_eq!(event.event, kinds::REQUEST);
        assert_eq!(
            event.payload,
            Some(json!({
                "id": "req-1",
                "kind": "permission",
                "title": "Bash make clean",
                "tool_name": "Bash",
                "input": { "command": "make clean" }
            }))
        );
        assert_eq!(pending.kind, RequestKind::Permission);
    }

    #[test]
    fn plan_approval_and_question_kinds_and_titles() {
        let (plan_event, plan) =
            request_from_control(&control("ExitPlanMode", json!({ "plan": "1. do\n2. it" })))
                .unwrap();
        assert_eq!(plan.kind, RequestKind::PlanApproval);
        assert_eq!(plan_event.payload.unwrap()["title"], "1. do 2. it");

        let (question_event, question) = request_from_control(&control(
            "AskUserQuestion",
            json!({ "questions": [{ "question": "Which db?", "options": [{ "label": "pg" }, { "label": "sqlite" }] }] }),
        ))
        .unwrap();
        assert_eq!(question.kind, RequestKind::Question);
        assert_eq!(
            question_event.payload.unwrap()["title"],
            "Which db? [pg / sqlite]"
        );
    }

    #[test]
    fn other_control_traffic_is_not_a_request() {
        let interrupt = json!({
            "type": "control_request",
            "request_id": "x",
            "request": { "subtype": "interrupt" }
        });
        assert!(request_from_control(&interrupt).is_none());
        assert!(request_from_control(&json!({ "type": "assistant" })).is_none());
    }

    #[test]
    fn permission_answers_map_to_allow_and_deny() {
        let (_, pending) = request_from_control(&bash_request()).unwrap();
        assert_eq!(
            control_response_for(&pending, &json!({ "allow": true })),
            json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": "req-1",
                    "response": { "behavior": "allow", "updatedInput": { "command": "make clean" } }
                }
            })
        );
        let denied =
            control_response_for(&pending, &json!({ "allow": false, "message": "too risky" }));
        assert_eq!(
            denied["response"]["response"],
            json!({ "behavior": "deny", "message": "too risky" })
        );
    }

    #[test]
    fn plan_rejection_carries_feedback_or_a_default() {
        let pending = PendingControl {
            cli_request_id: "req-2".into(),
            kind: RequestKind::PlanApproval,
            input: json!({ "plan": "p" }).as_object().unwrap().clone(),
        };
        let rejected = control_response_for(
            &pending,
            &json!({ "approved": false, "feedback": "split it up" }),
        );
        assert_eq!(
            rejected["response"]["response"],
            json!({ "behavior": "deny", "message": "split it up" })
        );
        let bare = control_response_for(&pending, &json!({ "approved": false }));
        assert_eq!(
            bare["response"]["response"]["message"],
            "The user rejected the plan."
        );
    }

    #[test]
    fn a_question_answer_is_attached_under_its_question_text() {
        let pending = PendingControl {
            cli_request_id: "req-3".into(),
            kind: RequestKind::Question,
            input: json!({ "questions": [{ "question": "Which db?" }] })
                .as_object()
                .unwrap()
                .clone(),
        };
        let reply = control_response_for(&pending, &json!({ "answer": "pg" }));
        assert_eq!(
            reply["response"]["response"],
            json!({
                "behavior": "allow",
                "updatedInput": {
                    "questions": [{ "question": "Which db?" }],
                    "answers": { "Which db?": "pg" }
                }
            })
        );
    }
}
