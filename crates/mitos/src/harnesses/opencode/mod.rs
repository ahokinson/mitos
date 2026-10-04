mod clients;
mod handoffs;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::Value;

use self::handoffs::collect_opencode_handoff;
use self::turns::{Turn, run_turn};
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

const PROGRAM: &str = "opencode";

pub struct OpenCodeAdapter;

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local")
                .join("share")
        },
        PathBuf::from,
    )
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

impl HarnessAdapter for OpenCodeAdapter {
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
            Some(session) => vec![
                "--session".into(),
                session.into(),
                "--prompt".into(),
                request.context.clone(),
            ],
            None => vec!["--prompt".into(), request.context.clone()],
        };
        Ok(LaunchPlan::new(
            PROGRAM,
            args,
            session.map(|session| Value::String(session.into())),
        ))
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        Ok(collect_opencode_handoff(&data_home(), request))
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
    use serde_json::json;

    use super::*;

    fn launch(native_session: Option<Value>) -> LaunchPlan {
        let request = AdapterRequest::prepare_launch(
            "opencode",
            Path::new("/w"),
            "ctx".into(),
            native_session,
        );
        OpenCodeAdapter.prepare_launch(&request).unwrap()
    }

    #[test]
    fn launches_interactively_with_the_context_as_its_prompt() {
        let fresh = launch(None);
        assert_eq!(fresh.program, "opencode");
        assert_eq!(fresh.args, ["--prompt", "ctx"]);
        let resumed = launch(Some(json!("ses_1")));
        assert_eq!(resumed.args, ["--session", "ses_1", "--prompt", "ctx"]);
        assert_eq!(resumed.native_session, Some(json!("ses_1")));
    }

    #[test]
    fn is_headless_with_plan_and_build_modes_and_ask_back() {
        let capabilities = OpenCodeAdapter.negotiate("opencode").unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(capabilities.ask_back);
    }
}
