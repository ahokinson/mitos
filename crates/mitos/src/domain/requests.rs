use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{HarnessKind, RequestId, ThreadId, Timestamp, TurnId};

string_enum! {
    enum RequestKind("request kind") {
        Question => "question",
        Permission => "permission",
        PlanApproval => "plan_approval",
    }
}

string_enum! {
    enum RequestStatus("request status") {
        Pending => "pending",
        Answered => "answered",
        Cancelled => "cancelled",
    }
}

/// What a person chose to tell a harness request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reply {
    Approve,
    Deny(Option<String>),
    Text(String),
}

impl RequestKind {
    /// The response an adapter expects for a reply to this kind of request.
    pub fn response_to(self, reply: &Reply) -> Result<Value> {
        match (self, reply) {
            (Self::Permission, Reply::Approve) => Ok(json!({ "allow": true })),
            (Self::Permission, Reply::Deny(note)) => Ok(with_note(
                json!({ "allow": false }),
                "message",
                note.as_deref(),
            )),
            (Self::PlanApproval, Reply::Approve) => Ok(json!({ "approved": true })),
            (Self::PlanApproval, Reply::Deny(note)) => Ok(with_note(
                json!({ "approved": false }),
                "feedback",
                note.as_deref(),
            )),
            (Self::Question, Reply::Text(text)) => Ok(json!({ "answer": text.trim() })),
            (Self::Question, _) => bail!("a question needs a text answer"),
            (_, Reply::Text(_)) => bail!("a {self} request takes approve or deny, not text"),
        }
    }
}

fn with_note(mut response: Value, key: &str, note: Option<&str>) -> Value {
    if let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) {
        response[key] = Value::String(note.to_owned());
    }
    response
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessRequest {
    pub id: RequestId,
    pub thread_id: ThreadId,
    pub turn_id: Option<TurnId>,
    pub harness: Option<HarnessKind>,
    pub kind: RequestKind,
    pub payload: Option<Value>,
    pub status: RequestStatus,
    pub response: Option<Value>,
    pub created_at: Timestamp,
    pub answered_at: Option<Timestamp>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Reply, RequestKind};

    fn deny(note: &str) -> Reply {
        Reply::Deny(Some(note.into()))
    }

    #[test]
    fn a_permission_is_allowed_or_denied_with_an_optional_message() {
        let kind = RequestKind::Permission;
        assert_eq!(
            kind.response_to(&Reply::Approve).unwrap(),
            json!({ "allow": true })
        );
        assert_eq!(
            kind.response_to(&deny(" no ")).unwrap(),
            json!({ "allow": false, "message": "no" })
        );
        assert_eq!(
            kind.response_to(&Reply::Deny(None)).unwrap(),
            json!({ "allow": false })
        );
        assert_eq!(
            kind.response_to(&deny("  ")).unwrap(),
            json!({ "allow": false })
        );
    }

    #[test]
    fn a_plan_is_approved_or_denied_with_optional_feedback() {
        let kind = RequestKind::PlanApproval;
        assert_eq!(
            kind.response_to(&Reply::Approve).unwrap(),
            json!({ "approved": true })
        );
        assert_eq!(
            kind.response_to(&deny("smaller")).unwrap(),
            json!({ "approved": false, "feedback": "smaller" })
        );
        assert_eq!(
            kind.response_to(&Reply::Deny(None)).unwrap(),
            json!({ "approved": false })
        );
    }

    #[test]
    fn a_question_takes_trimmed_text_and_nothing_else() {
        let kind = RequestKind::Question;
        assert_eq!(
            kind.response_to(&Reply::Text("  blue ".into())).unwrap(),
            json!({ "answer": "blue" })
        );
        for reply in [Reply::Approve, Reply::Deny(None)] {
            let error = kind.response_to(&reply).unwrap_err();
            assert!(error.to_string().contains("needs a text answer"));
        }
    }

    #[test]
    fn approvals_do_not_take_text() {
        for kind in [RequestKind::Permission, RequestKind::PlanApproval] {
            let error = kind.response_to(&Reply::Text("yes".into())).unwrap_err();
            assert!(error.to_string().contains("approve or deny"));
        }
    }
}
