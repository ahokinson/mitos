use serde::{Deserialize, Serialize};
use serde_json::Value;

string_enum! {
    enum UnbindReason("unbind reason") {
        Reassigned => "reassigned",
        Archived => "archived",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessBinding {
    pub id: String,
    pub thread_id: String,
    pub harness: String,
    pub native_session: Option<Value>,
    pub bound_at: String,
    pub unbound_at: Option<String>,
    pub unbind_reason: Option<UnbindReason>,
}
