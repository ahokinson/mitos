mod requests;
mod streams;
mod tokens;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::Value;

use self::turns::{Turn, run_turn};
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::history::{collect_requested_handoff, harness_home};
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

const PROGRAM: &str = "codex";

pub struct CodexAdapter;

fn sessions_root() -> PathBuf {
    harness_home("CODEX_HOME", ".codex").join("sessions")
}

fn turn_result(
    turn: &Turn<'_>,
    on_event: &mut EventSink<'_>,
    answers: Option<&Path>,
) -> Result<Option<Value>> {
    Emitter::collect(on_event, |emitter| {
        run_turn(OsStr::new(PROGRAM), turn, emitter, answers)
    })
}

impl HarnessAdapter for CodexAdapter {
    fn negotiate(&self, _harness: &str) -> Result<Capabilities> {
        Ok(Capabilities {
            headless: true,
            modes: vec![ThreadMode::Plan, ThreadMode::Build],
            ask_back: true,
        })
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        let session = string_value(request.native_session.as_ref());
        let args = match session {
            Some(session) => vec!["resume".into(), session.into(), request.context.clone()],
            None => vec![request.context.clone()],
        };
        Ok(LaunchPlan::new(
            PROGRAM,
            args,
            session.map(|session| Value::String(session.into())),
        ))
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        collect_requested_handoff(&sessions_root(), request)
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
            mode: request.mode,
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
            AdapterRequest::prepare_launch("codex", Path::new("/w"), "ctx".into(), native_session);
        CodexAdapter.prepare_launch(&request).unwrap()
    }

    #[test]
    fn launches_with_the_context_or_resumes_a_linked_session() {
        let fresh = launch(None);
        assert_eq!(fresh.program, "codex");
        assert_eq!(fresh.args, ["ctx"]);
        let resumed = launch(Some(json!("th-1")));
        assert_eq!(resumed.args, ["resume", "th-1", "ctx"]);
        assert_eq!(resumed.native_session, Some(json!("th-1")));
    }

    #[test]
    fn is_headless_with_plan_and_build_modes_and_ask_back() {
        let capabilities = CodexAdapter.negotiate("codex").unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(capabilities.ask_back);
    }
}
