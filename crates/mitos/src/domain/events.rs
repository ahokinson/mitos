use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{HarnessKind, ThreadId, Timestamp, TurnId};

string_enum! {
    /// Mirrors the `thread_events.kind` CHECK constraint in `0001_init.sql`.
    enum EventKind("event kind") {
        UserMessage => "user_message",
        AssistantMessage => "assistant_message",
        AssistantDelta => "assistant_delta",
        ToolCall => "tool_call",
        ToolResult => "tool_result",
        Status => "status",
        Usage => "usage",
        Error => "error",
        Note => "note",
        Decision => "decision",
        Question => "question",
        RequestOpened => "request_opened",
        RequestAnswered => "request_answered",
        ModeChanged => "mode_changed",
        ThreadCreated => "thread_created",
        HarnessBound => "harness_bound",
        HarnessUnbound => "harness_unbound",
        HandoffCarryover => "handoff_carryover",
        Compaction => "compaction",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadEvent {
    pub thread_id: ThreadId,
    pub seq: i64,
    pub turn_id: Option<TurnId>,
    pub harness: Option<HarnessKind>,
    pub kind: EventKind,
    pub role: Option<String>,
    pub content: Option<String>,
    pub payload: Option<Value>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Default)]
pub struct NewThreadEvent {
    pub turn_id: Option<TurnId>,
    pub harness: Option<HarnessKind>,
    pub kind: Option<EventKind>,
    pub role: Option<String>,
    pub content: Option<String>,
    pub payload: Option<Value>,
}

impl NewThreadEvent {
    pub fn new(kind: EventKind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::default()
        }
    }
}
