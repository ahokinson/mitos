use anyhow::{Result, bail};

use super::ThreadService;
use crate::domain::{EventKind, HarnessKind, NewThreadEvent, ThreadId};

pub struct ThreadNotes {
    pub note: Option<String>,
    pub decisions: Vec<String>,
    pub questions: Vec<String>,
}

impl ThreadNotes {
    fn is_empty(&self) -> bool {
        self.note.is_none() && self.decisions.is_empty() && self.questions.is_empty()
    }
}

impl ThreadService<'_> {
    pub fn note(&self, thread_id: &ThreadId, notes: ThreadNotes) -> Result<()> {
        if notes.is_empty() {
            bail!("provide --note, --decision, or --question");
        }
        self.record_notes(thread_id, None, notes)
    }

    pub(super) fn record_notes(
        &self,
        thread_id: &ThreadId,
        harness: Option<HarnessKind>,
        notes: ThreadNotes,
    ) -> Result<()> {
        let entries = notes
            .note
            .into_iter()
            .map(|content| (EventKind::Note, content))
            .chain(
                notes
                    .decisions
                    .into_iter()
                    .map(|content| (EventKind::Decision, content)),
            )
            .chain(
                notes
                    .questions
                    .into_iter()
                    .map(|content| (EventKind::Question, content)),
            );
        for (kind, content) in entries {
            self.store.append_event(
                thread_id,
                NewThreadEvent {
                    harness,
                    content: Some(content),
                    ..NewThreadEvent::new(kind)
                },
            )?;
        }
        Ok(())
    }
}
