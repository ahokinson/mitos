use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use super::ThreadService;
use super::usage::{plan_usage_fields, usage_fields};
use crate::domain::{EventKind, HarnessKind, NewThreadEvent, Thread, ThreadEvent, ThreadId, now};
use crate::handoff::{HandoffFacts, bounded_context, files_touched};
use crate::ports::HarnessAdapter;
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::CollectedHandoff;

type RecordedMessages = HashMap<(&'static str, String), usize>;

impl ThreadService<'_> {
    pub(super) fn render_handoff(&self, thread: &Thread) -> Result<String> {
        self.render_bounded(thread, self.handoff_limits.inline_bytes)
    }

    /// For `mitos enter`, where the handoff rides on the harness command line.
    pub(super) fn render_launch_handoff(&self, thread: &Thread) -> Result<String> {
        self.render_bounded(thread, self.handoff_limits.launch_bytes)
    }

    /// What the next harness needs to hear: everything from the latest
    /// compaction on, since the summary covers what came before.
    pub(super) fn handoff_events(&self, thread_id: &ThreadId) -> Result<Vec<ThreadEvent>> {
        let mut events = self.store.events_since(thread_id, 0)?;
        if let Some(start) = events
            .iter()
            .rposition(|event| event.kind == EventKind::Compaction)
        {
            events.drain(..start);
        }
        Ok(events)
    }

    fn render_bounded(&self, thread: &Thread, max_bytes: usize) -> Result<String> {
        let events = self.handoff_events(&thread.id)?;
        let workspace = self.store.get_workspace(&thread.workspace_id)?;
        let facts = HandoffFacts {
            thread_id: thread.id.clone(),
            files_touched: files_touched(&events, &workspace.root),
            events,
        };
        Ok(bounded_context(
            self.renderer.render(&facts, max_bytes)?,
            max_bytes,
        ))
    }

    /// Collection errors propagate before any binding changes.
    pub(super) fn collect_handoff_evidence<A: HarnessAdapter>(
        &self,
        thread: &Thread,
        workspace_root: &str,
        adapter: &A,
    ) -> Result<()> {
        let Some(harness) = thread.active_harness else {
            return Ok(());
        };
        if thread.native_session.is_none() {
            return Ok(());
        }
        let handoff = adapter.collect_handoff(&HandoffRequest::collect(
            harness,
            Path::new(workspace_root),
            now(),
            thread.native_session.clone(),
        ))?;
        let usage = handoff.usage.as_ref().map(usage_fields);
        let plan_usage = handoff.usage.as_ref().and_then(plan_usage_fields);
        if let Some(native_session) = handoff.native_session.as_ref() {
            self.store
                .set_thread_harness(&thread.id, Some(harness), Some(native_session))?;
        }
        self.record_transcript(thread, harness, transcript_events(handoff))?;
        if let Some(usage) = usage {
            self.store
                .record_usage_snapshot(&thread.id, harness, None, usage)?;
        }
        if let Some(plan_usage) = plan_usage {
            self.store.upsert_plan_usage(harness, plan_usage)?;
        }
        Ok(())
    }

    fn record_transcript(
        &self,
        thread: &Thread,
        harness: HarnessKind,
        events: Vec<NewThreadEvent>,
    ) -> Result<()> {
        let mut recorded = self.recorded_messages(&thread.id)?;
        for mut event in events {
            let key = event
                .kind
                .zip(event.content.as_deref())
                .map(|(kind, content)| (kind.as_str(), content.trim().to_owned()));
            if let Some(count) = key.and_then(|key| recorded.get_mut(&key))
                && *count > 0
            {
                *count -= 1;
                continue;
            }
            event.harness = Some(harness);
            self.store.append_event(&thread.id, event)?;
        }
        Ok(())
    }

    fn recorded_messages(&self, thread_id: &ThreadId) -> Result<RecordedMessages> {
        let mut recorded = RecordedMessages::new();
        for existing in self.store.events_since(thread_id, 0)? {
            if matches!(
                existing.kind,
                EventKind::UserMessage | EventKind::AssistantMessage
            ) && let Some(content) = &existing.content
            {
                *recorded
                    .entry((existing.kind.as_str(), content.trim().to_owned()))
                    .or_default() += 1;
            }
        }
        Ok(recorded)
    }
}

fn transcript_events(handoff: CollectedHandoff) -> Vec<NewThreadEvent> {
    handoff
        .transcript
        .map(|transcript| {
            transcript
                .messages
                .into_iter()
                .map(|message| {
                    let kind = if message.role == "assistant" {
                        EventKind::AssistantMessage
                    } else {
                        EventKind::UserMessage
                    };
                    NewThreadEvent {
                        role: Some(message.role),
                        content: Some(message.text),
                        ..NewThreadEvent::new(kind)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
