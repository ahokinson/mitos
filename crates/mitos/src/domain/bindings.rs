use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{HarnessKind, ThreadId, Timestamp};

string_enum! {
    enum UnbindReason("unbind reason") {
        Reassigned => "reassigned",
        Archived => "archived",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessBinding {
    pub id: String,
    pub thread_id: ThreadId,
    pub harness: HarnessKind,
    pub native_session: Option<Value>,
    pub bound_at: Timestamp,
    pub unbound_at: Option<Timestamp>,
    pub unbind_reason: Option<UnbindReason>,
}
