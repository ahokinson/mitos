use serde::{Deserialize, Serialize};
use serde_json::Value;

string_enum! {
    enum ThreadStatus("thread status") {
        Active => "active",
        Paused => "paused",
        Archived => "archived",
    }
}

string_enum! {
    #[derive(Default)]
    enum ThreadMode("thread mode") {
        Plan => "plan",
        #[default]
        Build => "build",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub workspace_id: String,
    pub status: ThreadStatus,
    pub mode: ThreadMode,
    pub active_harness: Option<String>,
    /// Opaque, adapter-owned.
    pub native_session: Option<Value>,
    pub last_event_seq: i64,
    pub created_at: String,
    pub updated_at: String,
}
