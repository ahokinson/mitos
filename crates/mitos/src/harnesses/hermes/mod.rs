mod handoffs;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Result, bail};
use serde_json::Value;

use self::handoffs::collect_hermes_handoff;
use self::turns::{Turn, run_turn};
use crate::adapters::Emitter;
use crate::history::harness_home;
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

const PROGRAM: &str = "hermes";

/// Interactive hermes cannot take an initial prompt and keep its REPL, so the
/// handoff is printed by Mitos and a linked session resumes in the Mitos
/// workspace. Headless turns run over `hermes acp`; no plan mode is offered.
pub struct HermesAdapter;

fn turn_result(
    turn: &Turn<'_>,
    on_event: &mut EventSink<'_>,
    answers: Option<&Path>,
) -> Result<Option<Value>> {
    Emitter::collect(on_event, |emitter| {
        run_turn(OsStr::new(PROGRAM), turn, emitter, answers)
    })
}

impl HarnessAdapter for HermesAdapter {
    fn negotiate(&self, _harness: &str) -> Result<Capabilities> {
        Ok(Capabilities {
            headless: true,
            modes: Vec::new(),
            ask_back: true,
        })
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        let session = string_value(request.native_session.as_ref());
        let args = match session {
            Some(session) => vec!["--resume".into(), session.into(), "--no-restore-cwd".into()],
            None => Vec::new(),
        };
        Ok(LaunchPlan::new(
            PROGRAM,
            args,
            session.map(|session| Value::String(session.into())),
        ))
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        let home = harness_home("HERMES_HOME", ".hermes");
        Ok(collect_hermes_handoff(&home, request))
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
            ephemeral: request.ephemeral,
        };
        turn_result(&turn, on_event, answers)
    }

    fn attach_thread(
        &self,
        _request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        Emitter::collect(on_event, |emitter| {
            emitter.emit(AdapterEvent::new(kinds::TURN_COMPLETE))
        })?;
        Ok(())
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
            ephemeral: false,
        };
        turn_result(&turn, on_event, answers)?;
        Ok(())
    }

    fn detach_thread(&self, _request: &DetachThreadRequest) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn launch(native_session: Option<Value>) -> LaunchPlan {
        let request =
            AdapterRequest::prepare_launch("hermes", Path::new("/w"), "ctx".into(), native_session);
        HermesAdapter.prepare_launch(&request).unwrap()
    }

    #[test]
    fn resumes_a_linked_session_and_never_takes_the_context_as_an_arg() {
        let fresh = launch(None);
        assert_eq!(fresh.program, "hermes");
        assert!(fresh.args.is_empty());
        assert_eq!(fresh.native_session, None);
        let resumed = launch(Some(json!("sess-1")));
        assert_eq!(resumed.args, ["--resume", "sess-1", "--no-restore-cwd"]);
        assert_eq!(resumed.native_session, Some(json!("sess-1")));
    }

    #[test]
    fn is_headless_and_build_only_so_plan_reassignment_is_refused() {
        let capabilities = HermesAdapter.negotiate("hermes").unwrap();
        assert!(capabilities.headless);
        assert!(capabilities.modes.is_empty());
        assert!(capabilities.ask_back);
    }
}
