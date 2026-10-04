use std::path::Path;

use anyhow::{Result, bail};
use serde_json::Value;

use super::{Emitter, Turn};
use crate::domain::HarnessKind;
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

/// What differs between native harnesses; `HarnessAdapter` is derived from it.
pub trait Harness {
    const PROGRAM: &'static str;

    fn capabilities(&self) -> Capabilities;

    /// Arguments for the interactive harness, resuming `session` when given.
    fn launch_args(&self, session: Option<&str>, context: &str) -> Vec<String>;

    fn run_turn(
        &self,
        turn: &Turn<'_>,
        emitter: &mut Emitter<'_, '_>,
        answers: Option<&Path>,
    ) -> Result<()>;

    fn collect(&self, request: &HandoffRequest) -> Result<CollectedHandoff>;

    /// Harnesses that can replay their history override this.
    fn attach(&self, _request: &AttachThreadRequest, on_event: &mut EventSink<'_>) -> Result<()> {
        Emitter::collect(on_event, |emitter| {
            emitter.emit(AdapterEvent::new(kinds::TURN_COMPLETE))
        })?;
        Ok(())
    }
}

impl<H: Harness> HarnessAdapter for H {
    fn negotiate(&self, _harness: HarnessKind) -> Result<Capabilities> {
        Ok(self.capabilities())
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        let session = string_value(request.native_session.as_ref());
        Ok(LaunchPlan::new(
            H::PROGRAM,
            self.launch_args(session, &request.context),
            session.map(|session| Value::String(session.into())),
        ))
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        self.collect(request)
    }

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        let turn = Turn {
            workdir: &request.workdir,
            text: &request.initial_context,
            session: None,
            mode: request.mode,
            ephemeral: request.ephemeral,
        };
        Emitter::collect(on_event, |emitter| self.run_turn(&turn, emitter, answers))
    }

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        self.attach(request, on_event)
    }

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<()> {
        let Some(session) = string_value(request.native_session.as_ref()) else {
            bail!("send_message requires a native_session");
        };
        let turn = Turn {
            workdir: &request.workdir,
            text: &request.text,
            session: Some(session),
            mode: request.mode,
            ephemeral: false,
        };
        Emitter::collect(on_event, |emitter| self.run_turn(&turn, emitter, answers))?;
        Ok(())
    }

    fn detach_thread(&self, _request: &DetachThreadRequest) -> Result<()> {
        Ok(())
    }
}
