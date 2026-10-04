use anyhow::Result;
use serde::Serialize;

use super::ThreadService;
use crate::diffs::{FileDiff, payload_diffs};
use crate::domain::{EventKind, HarnessKind, ThreadEvent, ThreadId, ThreadSummary, ThreadUsage};
use crate::tools::{ToolFacts, tool_facts};

/// A synced event with what its tool payload says, so a frontend never reads
/// the payload.
#[derive(Debug, Serialize)]
pub struct EventView {
    #[serde(flatten)]
    pub event: ThreadEvent,
    pub diffs: Vec<FileDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolFacts>,
}

/// Read-only views of stored state for frontends.
impl ThreadService<'_> {
    pub fn event_views(&self, thread_id: &ThreadId, since_seq: i64) -> Result<Vec<EventView>> {
        Ok(self
            .sync(thread_id, since_seq)?
            .into_iter()
            .map(|event| {
                let diffs = match (&event.kind, &event.payload) {
                    (EventKind::ToolCall | EventKind::ToolResult, Some(payload)) => {
                        payload_diffs(payload)
                    }
                    _ => Vec::new(),
                };
                let tool = tool_facts(event.kind, event.payload.as_ref(), event.content.as_deref());
                EventView { event, diffs, tool }
            })
            .collect())
    }

    pub fn thread_summaries(&self, workspace_key: &str) -> Result<Vec<ThreadSummary>> {
        self.store.thread_summaries(workspace_key)
    }

    pub fn recent_user_messages(&self, workspace_key: &str, limit: usize) -> Result<Vec<String>> {
        self.store.recent_user_messages(workspace_key, limit)
    }

    pub fn is_thread_empty(&self, thread_id: &ThreadId) -> Result<bool> {
        self.store.is_thread_empty(thread_id)
    }

    pub fn latest_usage(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
    ) -> Result<Option<ThreadUsage>> {
        self.store.latest_usage(thread_id, harness)
    }
}
