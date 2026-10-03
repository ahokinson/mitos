use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::ThreadService;
use super::bindings::ensure_supported;
use super::usage::{plan_usage_fields, usage_fields};
use crate::domain::{EventKind, NewThreadEvent, Thread, id};
use crate::ipc::answers;
use crate::ports::HarnessAdapter;
use crate::wire::events::AdapterEvent;
use crate::wire::requests::{AttachThreadRequest, SendMessageRequest, StartThreadRequest};

struct Turn<'a> {
    thread: Thread,
    harness: String,
    id: &'a str,
    socket: Option<PathBuf>,
}

impl ThreadService<'_> {
    pub fn send<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        text: String,
        adapter: &A,
    ) -> Result<String> {
        let thread = self.store.get_thread(thread_id)?;
        thread
            .active_harness
            .as_ref()
            .context("thread has no harness assigned; run `mitos thread reassign` first")?;
        let turn_id = id();
        self.store.append_event(
            thread_id,
            NewThreadEvent {
                turn_id: Some(turn_id.clone()),
                role: Some("user".into()),
                content: Some(text),
                ..NewThreadEvent::new(EventKind::UserMessage)
            },
        )?;
        self.drive(thread_id, &turn_id, adapter)?;
        Ok(turn_id)
    }

    pub fn drive<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        turn_id: &str,
        adapter: &A,
    ) -> Result<()> {
        let _lock = self.store.lock_thread(thread_id)?;
        let thread = self.store.get_thread(thread_id)?;
        let harness = thread
            .active_harness
            .clone()
            .context("thread has no harness assigned")?;
        let capabilities = adapter.negotiate(&harness)?;
        ensure_supported(&harness, &capabilities, thread.mode)?;
        let turn = Turn {
            thread,
            harness,
            id: turn_id,
            socket: capabilities
                .ask_back
                .then(|| answers::socket_path(self.store.config_root(), thread_id)),
        };
        match turn.thread.native_session.clone() {
            Some(native_session) => self.resume_turn(&turn, native_session, adapter),
            None => self.start_turn(&turn, adapter),
        }
    }

    fn resume_turn<A: HarnessAdapter>(
        &self,
        turn: &Turn<'_>,
        native_session: Value,
        adapter: &A,
    ) -> Result<()> {
        let thread_id = &turn.thread.id;
        let workspace = self.store.get_workspace(&turn.thread.workspace_id)?;
        let text = self.store.user_message_for_turn(thread_id, turn.id)?;
        let request = SendMessageRequest::new(
            &turn.harness,
            thread_id,
            PathBuf::from(&workspace.root),
            Some(native_session),
            turn.thread.mode,
            turn.id,
            text,
        );
        adapter.send_message(
            &request,
            &mut |event| self.record_adapter_event(thread_id, &turn.harness, turn.id, event),
            turn.socket.as_deref(),
        )
    }

    fn start_turn<A: HarnessAdapter>(&self, turn: &Turn<'_>, adapter: &A) -> Result<()> {
        let thread_id = &turn.thread.id;
        let workspace = self.store.get_workspace(&turn.thread.workspace_id)?;
        let context = self.render_handoff(&turn.thread)?;
        let request = StartThreadRequest::new(
            &turn.harness,
            thread_id,
            PathBuf::from(&workspace.root),
            turn.thread.mode,
            context,
        );
        let native_session = adapter.start_thread(
            &request,
            &mut |event| self.record_adapter_event(thread_id, &turn.harness, turn.id, event),
            turn.socket.as_deref(),
        )?;
        if let Some(native_session) = native_session {
            self.bind_harness(thread_id, &turn.harness, Some(&native_session))?;
        }
        Ok(())
    }

    pub fn attach<A: HarnessAdapter>(&self, thread_id: &str, adapter: &A) -> Result<()> {
        let _lock = self.store.lock_thread(thread_id)?;
        let thread = self.store.get_thread(thread_id)?;
        let Some(harness) = thread.active_harness.clone() else {
            return Ok(());
        };
        if !adapter.negotiate(&harness)?.headless {
            return Ok(());
        }
        let workspace = self.store.get_workspace(&thread.workspace_id)?;
        let turn_id = id();
        let request = AttachThreadRequest::new(
            &harness,
            thread_id,
            PathBuf::from(&workspace.root),
            thread.native_session.clone(),
            thread.last_event_seq,
        );
        adapter.attach_thread(&request, &mut |event| {
            self.record_adapter_event(thread_id, &harness, &turn_id, event)
        })
    }

    fn record_adapter_event(
        &self,
        thread_id: &str,
        harness: &str,
        turn_id: &str,
        event: AdapterEvent,
    ) -> Result<()> {
        if event.event == "request" {
            return self.open_adapter_request(thread_id, harness, turn_id, event);
        }
        let kind = match event.event.as_str() {
            "assistant_delta" => EventKind::AssistantDelta,
            "assistant_message" => EventKind::AssistantMessage,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "status" | "native_session_update" | "turn_complete" => EventKind::Status,
            "usage" => EventKind::Usage,
            "error" => EventKind::Error,
            other => bail!("adapter emitted unknown event kind {other:?}"),
        };
        if let Some(usage) = &event.usage {
            self.store.record_usage_snapshot(
                thread_id,
                harness,
                Some(turn_id),
                usage_fields(usage),
            )?;
            if let Some(plan_fields) = plan_usage_fields(usage) {
                self.store.upsert_plan_usage(harness, plan_fields)?;
            }
        }
        self.store.append_event(
            thread_id,
            NewThreadEvent {
                turn_id: Some(turn_id.into()),
                harness: Some(harness.into()),
                role: event.role,
                content: event.content,
                payload: event.payload,
                ..NewThreadEvent::new(kind)
            },
        )?;
        Ok(())
    }
}
