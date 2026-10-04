use anyhow::Result;
use libsql::params;

use super::Store;
#[cfg(test)]
use super::queries::{parse_optional, text};
use crate::domain::{HandoffCarryover, HarnessKind, ThreadId, id, now};

#[derive(Clone, Debug)]
pub struct NewHandoffCarryover {
    pub thread_id: ThreadId,
    pub from_harness: Option<HarnessKind>,
    pub to_harness: HarnessKind,
    pub note: Option<String>,
    pub decisions: Vec<String>,
    pub questions: Vec<String>,
    pub bounded_context: String,
    pub source_event_seq_high_watermark: Option<i64>,
}

impl Store {
    pub fn record_handoff_carryover(&self, new: NewHandoffCarryover) -> Result<HandoffCarryover> {
        let carryover = HandoffCarryover {
            id: id(),
            thread_id: new.thread_id,
            from_harness: new.from_harness,
            to_harness: new.to_harness,
            created_at: now(),
            note: new.note,
            decisions: new.decisions,
            questions: new.questions,
            bounded_context: new.bounded_context,
            source_event_seq_high_watermark: new.source_event_seq_high_watermark,
        };
        self.execute(
            "INSERT INTO handoff_carryovers \
             (id, thread_id, from_harness, to_harness, created_at, note, decisions_json, questions_json, bounded_context, source_event_seq_high_watermark) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                carryover.id.clone(),
                carryover.thread_id.as_str(),
                carryover.from_harness.map(HarnessKind::as_str),
                carryover.to_harness.as_str(),
                carryover.created_at.as_str(),
                carryover.note.clone(),
                serde_json::to_string(&carryover.decisions)?,
                serde_json::to_string(&carryover.questions)?,
                carryover.bounded_context.clone(),
                carryover.source_event_seq_high_watermark
            ],
        )?;
        Ok(carryover)
    }

    #[cfg(test)]
    pub fn handoff_carryovers(&self, thread_id: &ThreadId) -> Result<Vec<HandoffCarryover>> {
        self.query_all(
            "SELECT id, thread_id, from_harness, to_harness, created_at, note, decisions_json, questions_json, bounded_context, source_event_seq_high_watermark \
             FROM handoff_carryovers WHERE thread_id = ?1 ORDER BY created_at ASC, id ASC",
            params![thread_id.as_str()],
            |row| {
                Ok(HandoffCarryover {
                    id: row.get(0)?,
                    thread_id: text(row, 1)?,
                    from_harness: parse_optional(row.get(2)?)?,
                    to_harness: row.get::<String>(3)?.parse()?,
                    created_at: text(row, 4)?,
                    note: row.get(5)?,
                    decisions: serde_json::from_str(&row.get::<String>(6)?)?,
                    questions: serde_json::from_str(&row.get::<String>(7)?)?,
                    bounded_context: row.get(8)?,
                    source_event_seq_high_watermark: row.get(9)?,
                })
            },
        )
    }
}
