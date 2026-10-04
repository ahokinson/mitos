mod controls;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;

use self::turns::run_turn;
use crate::domain::ThreadMode;
use crate::harnesses::{Emitter, Harness, Turn};
use crate::history::{JsonlSearch, collect_jsonl_handoff, collect_requested_handoff, harness_home};
use crate::ports::EventSink;
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{AttachThreadRequest, HandoffRequest};
use crate::wire::responses::{Capabilities, CollectedHandoff};

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

impl Harness for ClaudeAdapter {
    const PROGRAM: &'static str = "claude";

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            headless: true,
            modes: vec![ThreadMode::Plan, ThreadMode::Build],
            ask_back: false,
        }
    }

    fn launch_args(&self, session: Option<&str>, context: &str) -> Vec<String> {
        match session {
            Some(session) => vec!["--resume".into(), session.into(), context.into()],
            None => vec![context.into()],
        }
    }

    fn run_turn(
        &self,
        turn: &Turn<'_>,
        emitter: &mut Emitter<'_, '_>,
        answers: Option<&Path>,
    ) -> Result<()> {
        run_turn(OsStr::new(Self::PROGRAM), turn, emitter, answers)
    }

    fn collect(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        collect_requested_handoff(&projects_root(), request)
    }

    fn attach(&self, request: &AttachThreadRequest, on_event: &mut EventSink<'_>) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::HarnessKind;
    use crate::ports::HarnessAdapter;
    use crate::wire::requests::AdapterRequest;
    use crate::wire::responses::LaunchPlan;

    fn launch(native_session: Option<Value>) -> LaunchPlan {
        let request =
            AdapterRequest::prepare_launch(HarnessKind::Claude, "ctx".into(), native_session);
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
        let capabilities = ClaudeAdapter.negotiate(HarnessKind::Claude).unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(!capabilities.ask_back);
    }
}
