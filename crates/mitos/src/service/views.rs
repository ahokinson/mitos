use anyhow::Result;

use super::ThreadService;
use crate::domain::{ThreadSummary, ThreadUsage};

/// Read-only views of stored state for frontends.
impl ThreadService<'_> {
    pub fn thread_summaries(&self, workspace_key: &str) -> Result<Vec<ThreadSummary>> {
        self.store.thread_summaries(workspace_key)
    }

    pub fn recent_user_messages(&self, workspace_key: &str, limit: usize) -> Result<Vec<String>> {
        self.store.recent_user_messages(workspace_key, limit)
    }

    pub fn is_thread_empty(&self, thread_id: &str) -> Result<bool> {
        self.store.is_thread_empty(thread_id)
    }

    pub fn latest_usage(&self, thread_id: &str, harness: &str) -> Result<Option<ThreadUsage>> {
        self.store.latest_usage(thread_id, harness)
    }
}
