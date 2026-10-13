use std::collections::HashSet;

use anyhow::Result;
use libsql::params;

use super::super::Store;
use super::super::threads::thread_from_row;
use crate::domain::{EventKind, ThreadId, ThreadSummary};

/// The events a thread records about itself before anything is said.
const SETUP_EVENTS: [EventKind; 4] = [
    EventKind::ThreadCreated,
    EventKind::HarnessBound,
    EventKind::HarnessUnbound,
    EventKind::ModeChanged,
];

impl Store {
    /// A workspace's threads, newest first, without the archived ones.
    pub fn thread_summaries(&self, workspace_key: &str) -> Result<Vec<ThreadSummary>> {
        self.query_all(
            "SELECT threads.id, threads.workspace_id, threads.status, threads.mode, \
                    threads.agent, threads.model, \
                    threads.active_harness, threads.native_session, threads.last_event_seq, \
                    threads.created_at, threads.updated_at, \
                    (SELECT content FROM thread_events \
                     WHERE thread_id = threads.id AND kind = 'user_message' AND content IS NOT NULL \
                     ORDER BY seq ASC LIMIT 1) \
             FROM threads JOIN workspaces ON workspaces.id = threads.workspace_id \
             WHERE workspaces.workspace_key = ?1 AND threads.status != 'archived' \
             ORDER BY threads.updated_at DESC",
            params![workspace_key],
            |row| {
                Ok(ThreadSummary {
                    thread: thread_from_row(row)?,
                    opening_message: row.get(11)?,
                })
            },
        )
    }

    /// Messages the user sent in a workspace, newest first, each text once.
    pub fn recent_user_messages(&self, workspace_key: &str, limit: usize) -> Result<Vec<String>> {
        let contents = self.query_all(
            "SELECT thread_events.content FROM thread_events \
             JOIN threads ON threads.id = thread_events.thread_id \
             JOIN workspaces ON workspaces.id = threads.workspace_id \
             WHERE workspaces.workspace_key = ?1 \
               AND thread_events.kind = 'user_message' \
               AND thread_events.role = 'user' \
               AND thread_events.content IS NOT NULL \
             ORDER BY thread_events.created_at DESC, thread_events.seq DESC",
            params![workspace_key],
            |row| Ok(row.get::<String>(0)?),
        )?;
        let mut seen = HashSet::new();
        let mut messages = Vec::new();
        for content in contents {
            if messages.len() >= limit {
                break;
            }
            let text = content.trim();
            if !text.is_empty() && seen.insert(text.to_string()) {
                messages.push(text.to_string());
            }
        }
        Ok(messages)
    }

    /// True only when the thread exists and has recorded nothing beyond its
    /// own setup events.
    pub fn is_thread_empty(&self, thread_id: &ThreadId) -> Result<bool> {
        let exists = self
            .query_optional(
                "SELECT 1 FROM threads WHERE id = ?1",
                params![thread_id.as_str()],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .is_some();
        let setup = SETUP_EVENTS
            .iter()
            .map(|kind| format!("'{}'", kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let substantive = self
            .query_optional(
                &format!(
                    "SELECT 1 FROM thread_events WHERE thread_id = ?1 \
                     AND kind NOT IN ({setup}) LIMIT 1"
                ),
                params![thread_id.as_str()],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .is_some();
        Ok(exists && !substantive)
    }
}
