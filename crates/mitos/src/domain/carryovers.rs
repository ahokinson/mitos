use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandoffCarryover {
    pub id: String,
    pub thread_id: String,
    pub from_harness: Option<String>,
    pub to_harness: String,
    pub created_at: String,
    pub note: Option<String>,
    pub decisions: Vec<String>,
    pub questions: Vec<String>,
    /// The text injected as the new harness's first-turn context.
    pub bounded_context: String,
    pub source_event_seq_high_watermark: Option<i64>,
}
