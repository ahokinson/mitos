mod requests;
mod streams;
mod tokens;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Result;

use self::turns::run_turn;
use crate::domain::ThreadMode;
use crate::harnesses::{Emitter, Harness, Turn};
use crate::history::{collect_requested_handoff, harness_home};
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{Capabilities, CollectedHandoff};

pub struct CodexAdapter;

fn sessions_root() -> PathBuf {
    harness_home("CODEX_HOME", ".codex").join("sessions")
}

impl Harness for CodexAdapter {
    const PROGRAM: &'static str = "codex";

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            headless: true,
            modes: vec![ThreadMode::Plan, ThreadMode::Build],
            ask_back: true,
        }
    }

    fn launch_args(&self, session: Option<&str>, context: &str) -> Vec<String> {
        match session {
            Some(session) => vec!["resume".into(), session.into(), context.into()],
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
        collect_requested_handoff(&sessions_root(), request)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::domain::HarnessKind;
    use crate::ports::HarnessAdapter;
    use crate::wire::requests::AdapterRequest;
    use crate::wire::responses::LaunchPlan;

    fn launch(native_session: Option<Value>) -> LaunchPlan {
        let request =
            AdapterRequest::prepare_launch(HarnessKind::Codex, "ctx".into(), native_session);
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
        let capabilities = CodexAdapter.negotiate(HarnessKind::Codex).unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(capabilities.ask_back);
    }
}
