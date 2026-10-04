mod controls;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;

use self::turns::{Turn, run_turn};
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::history::{JsonlSearch, collect_jsonl_handoff, collect_requested_handoff, harness_home};
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

const PROGRAM: &str = "claude";
const ASSISTANT: &str = "assistant";

pub struct ClaudeAdapter;

fn projects_root() -> PathBuf {
    harness_home("CLAUDE_CONFIG_DIR", ".claude").join("projects")
}

fn collect(
    workdir: &Path,
    launched_at: Option<DateTime<Utc>>,
    native_session: Option<&Value>,
) -> Result<CollectedHandoff> {
    collect_jsonl_handoff(&JsonlSearch {
        root: &projects_root(),
        workdir,
        launched_at,
        native_session,
    })
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

impl HarnessAdapter for ClaudeAdapter {
    fn negotiate(&self, _harness: &str) -> Result<Capabilities> {
        Ok(Capabilities {
            headless: true,
            modes: vec![ThreadMode::Plan, ThreadMode::Build],
            ask_back: false,
        })
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        let session = string_value(request.native_session.as_ref());
        let args = match session {
            Some(session) => vec!["--resume".into(), session.into(), request.context.clone()],
            None => vec![request.context.clone()],
        };
        Ok(LaunchPlan::new(
            PROGRAM,
            args,
            session.map(|session| Value::String(session.into())),
        ))
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        collect_requested_handoff(&projects_root(), request)
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
            resume: None,
            mode: request.mode,
        };
        turn_result(&turn, on_event, answers)
    }

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        let handoff = collect(
            &request.workdir,
            Some(DateTime::UNIX_EPOCH),
            request.native_session.as_ref(),
        )?;
        let mut emitter = Emitter::new(on_event);
        for message in handoff.transcript.into_iter().flat_map(|t| t.messages) {
            let kind = if message.role == ASSISTANT {
                kinds::ASSISTANT_MESSAGE
            } else {
                kinds::STATUS
            };
            emitter.emit(AdapterEvent {
                role: Some(message.role),
                content: Some(message.text),
                ..AdapterEvent::new(kind)
            })?;
        }
        if let Some(usage) = handoff.usage {
            emitter.emit(AdapterEvent {
                usage: Some(usage),
                ..AdapterEvent::new(kinds::USAGE)
            })?;
        }
        emitter.emit(AdapterEvent::new(kinds::TURN_COMPLETE))
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
            resume: Some(session),
            mode: request.mode,
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
        let request =
            AdapterRequest::prepare_launch("claude", Path::new("/w"), "ctx".into(), native_session);
        ClaudeAdapter.prepare_launch(&request).unwrap()
    }

    #[test]
    fn launches_with_the_context_or_resumes_a_linked_session() {
        let fresh = launch(None);
        assert_eq!(fresh.program, "claude");
        assert_eq!(fresh.args, ["ctx"]);
        let resumed = launch(Some(json!("s-1")));
        assert_eq!(resumed.args, ["--resume", "s-1", "ctx"]);
        assert_eq!(resumed.native_session, Some(json!("s-1")));
    }

    #[test]
    fn is_headless_with_plan_and_build_modes_and_no_ask_back() {
        let capabilities = ClaudeAdapter.negotiate("claude").unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(!capabilities.ask_back);
    }
}
