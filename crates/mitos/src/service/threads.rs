use std::path::Path;

use anyhow::{Context, Result, bail};

use super::ThreadService;
use super::bindings::ensure_supported;
use crate::domain::{
    CompactMode, EventKind, NewThreadEvent, Thread, ThreadEvent, ThreadMode, ThreadStatus,
    UnbindReason,
};
use crate::git::Checkout;
use crate::ports::HarnessAdapter;
use crate::store::NewHandoffCarryover;
use crate::wire::requests::DetachThreadRequest;

const SUMMARIZE_PROMPT: &str = "Summarize this conversation so it can continue in a fresh context. Cover the goal, decisions made, work completed, files touched, open questions and next steps. Reply with the summary only.";

impl ThreadService<'_> {
    pub fn new_thread(&self, harness: Option<String>, workspace_path: &Path) -> Result<Thread> {
        let checkout = Checkout::open(workspace_path)?;
        let workspace =
            self.store
                .workspace_for(checkout.root(), checkout.git_dir(), &checkout.key())?;
        let thread = self.store.create_thread(&workspace.id)?;
        if let Some(harness) = harness {
            self.bind_harness(&thread.id, &harness, None)?;
        }
        self.store.get_thread(&thread.id)
    }

    pub fn list_threads(&self, workspace_path: &Path) -> Result<Vec<Thread>> {
        let checkout = Checkout::open(workspace_path)?;
        let Some(workspace) = self.store.find_workspace_by_key(&checkout.key())? else {
            return Ok(Vec::new());
        };
        self.store.list_threads(&workspace.id)
    }

    pub fn sync(&self, thread_id: &str, since_seq: i64) -> Result<Vec<ThreadEvent>> {
        self.store.events_since(thread_id, since_seq)
    }

    pub fn set_mode(&self, thread_id: &str, mode: ThreadMode) -> Result<()> {
        if self.store.get_thread(thread_id)?.mode == mode {
            return Ok(());
        }
        self.store.set_thread_mode(thread_id, mode)
    }

    pub fn archive<A: HarnessAdapter>(&self, thread_id: &str, adapter: &A) -> Result<()> {
        let _lock = self.store.lock_thread(thread_id)?;
        let thread = self.store.get_thread(thread_id)?;
        if let Some(harness) = &thread.active_harness {
            adapter.detach_thread(&detach_request(harness, &thread))?;
            self.unbind_harness(thread_id, harness, UnbindReason::Archived)?;
        }
        self.store
            .set_thread_status(thread_id, ThreadStatus::Archived)
    }

    /// No unbind event: the event log is deleted with the thread.
    pub fn delete<A: HarnessAdapter>(&self, thread_id: &str, adapter: &A) -> Result<()> {
        let _lock = self.store.lock_thread(thread_id)?;
        let thread = self.store.get_thread(thread_id)?;
        if let Some(harness) = &thread.active_harness {
            adapter.detach_thread(&detach_request(harness, &thread))?;
        }
        self.store.delete_thread(thread_id)
    }

    pub fn reassign_harness<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        to_harness: &str,
        adapter: &A,
    ) -> Result<()> {
        self.rebind_harness(thread_id, to_harness, None, adapter)
    }

    pub fn compact_thread<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        mode: CompactMode,
        adapter: &A,
    ) -> Result<()> {
        let thread = self.store.get_thread(thread_id)?;
        let harness = thread
            .active_harness
            .context("thread has no harness assigned")?;
        let summary = match mode {
            CompactMode::Mechanical => None,
            CompactMode::Intelligent => {
                Some(self.summarize_conversation(thread_id, thread.last_event_seq, adapter)?)
            }
        };
        self.rebind_harness(thread_id, &harness, summary, adapter)
    }

    fn summarize_conversation<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        since_seq: i64,
        adapter: &A,
    ) -> Result<String> {
        let turn_id = self.send(thread_id, SUMMARIZE_PROMPT.into(), adapter)?;
        let summary = self
            .store
            .events_since(thread_id, since_seq)?
            .into_iter()
            .rev()
            .find(|event| {
                event.kind == EventKind::AssistantMessage
                    && event.turn_id.as_deref() == Some(turn_id.as_str())
            })
            .and_then(|event| event.content)
            .filter(|content| !content.trim().is_empty());
        match summary {
            Some(summary) => Ok(summary),
            None => bail!("the harness returned no summary; thread left unchanged"),
        }
    }

    fn rebind_harness<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        to_harness: &str,
        summary: Option<String>,
        adapter: &A,
    ) -> Result<()> {
        let _lock = self.store.lock_thread(thread_id)?;
        let thread = self.store.get_thread(thread_id)?;
        let capabilities = adapter.negotiate(to_harness)?;
        ensure_supported(to_harness, &capabilities, thread.mode)?;
        let workspace = self.store.get_workspace(&thread.workspace_id)?;
        self.collect_handoff_evidence(&thread, &workspace.root, adapter)?;
        self.store.cancel_pending_requests(thread_id)?;
        if let Some(summary) = summary {
            self.store.append_event(
                thread_id,
                NewThreadEvent {
                    harness: Some(to_harness.into()),
                    content: Some(summary),
                    ..NewThreadEvent::new(EventKind::Compaction)
                },
            )?;
        }
        let thread = self.store.get_thread(thread_id)?;
        let from_harness = thread.active_harness.clone();

        let context = self.render_handoff(&thread)?;
        self.store.record_handoff_carryover(NewHandoffCarryover {
            thread_id: thread_id.into(),
            from_harness: from_harness.clone(),
            to_harness: to_harness.into(),
            note: None,
            decisions: Vec::new(),
            questions: Vec::new(),
            bounded_context: context,
            source_event_seq_high_watermark: Some(thread.last_event_seq),
        })?;
        self.store.append_event(
            thread_id,
            NewThreadEvent {
                harness: Some(to_harness.into()),
                ..NewThreadEvent::new(EventKind::HandoffCarryover)
            },
        )?;

        if let Some(from) = &from_harness {
            self.unbind_harness(thread_id, from, UnbindReason::Reassigned)?;
        }

        self.store.append_event(
            thread_id,
            NewThreadEvent {
                harness: Some(to_harness.into()),
                ..NewThreadEvent::new(EventKind::HarnessBound)
            },
        )?;
        self.bind_harness(thread_id, to_harness, None)
    }
}

fn detach_request(harness: &str, thread: &Thread) -> DetachThreadRequest {
    DetachThreadRequest::new(harness, thread.native_session.clone())
}
