use serde::{Deserialize, Serialize};

use super::{HarnessKind, ThreadId, Timestamp};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandoffCarryover {
    pub id: String,
    pub thread_id: ThreadId,
    pub from_harness: Option<HarnessKind>,
    pub to_harness: HarnessKind,
    pub created_at: Timestamp,
    pub note: Option<String>,
    pub decisions: Vec<String>,
    pub questions: Vec<String>,
    /// The text injected as the new harness's first-turn context.
    pub bounded_context: String,
    pub source_event_seq_high_watermark: Option<i64>,
}
