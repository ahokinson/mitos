mod clients;
mod handoffs;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Result;

use self::handoffs::collect_opencode_handoff;
use self::turns::run_turn;
use crate::domain::ThreadMode;
use crate::harnesses::{Emitter, Harness, Turn};
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{Capabilities, CollectedHandoff};

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

impl Harness for OpenCodeAdapter {
    const PROGRAM: &'static str = "opencode";

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            headless: true,
            modes: vec![ThreadMode::Plan, ThreadMode::Build],
            ask_back: true,
        }
    }

    fn launch_args(&self, session: Option<&str>, context: &str) -> Vec<String> {
        match session {
            Some(session) => vec![
                "--session".into(),
                session.into(),
                "--prompt".into(),
                context.into(),
            ],
            None => vec!["--prompt".into(), context.into()],
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
        Ok(collect_opencode_handoff(&data_home(), request))
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
            AdapterRequest::prepare_launch(HarnessKind::OpenCode, "ctx".into(), native_session);
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
        let capabilities = OpenCodeAdapter.negotiate(HarnessKind::OpenCode).unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, [ThreadMode::Plan, ThreadMode::Build]);
        assert!(capabilities.ask_back);
    }
}
