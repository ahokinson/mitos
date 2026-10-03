use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessRequest {
    pub id: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub harness: Option<String>,
    pub kind: RequestKind,
    pub payload: Option<Value>,
    pub status: RequestStatus,
    pub response: Option<Value>,
    pub created_at: String,
    pub answered_at: Option<String>,
}
