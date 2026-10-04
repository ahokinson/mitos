use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{HarnessKind, ThreadId, Timestamp, WorkspaceId};

string_enum! {
    enum ThreadStatus("thread status") {
        Active => "active",
        Paused => "paused",
        Archived => "archived",
    }
}

string_enum! {
    enum CompactMode("compact mode") {
        Mechanical => "mechanical",
        Intelligent => "intelligent",
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

/// A thread as a frontend lists it.
#[derive(Clone, Debug, Serialize)]
pub struct ThreadSummary {
    #[serde(flatten)]
    pub thread: Thread,
    /// The first user message, which gives a thread a human-readable identity.
    pub opening_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub workspace_id: WorkspaceId,
    pub status: ThreadStatus,
    pub mode: ThreadMode,
    pub active_harness: Option<HarnessKind>,
    /// Opaque, adapter-owned.
    pub native_session: Option<Value>,
    pub last_event_seq: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
