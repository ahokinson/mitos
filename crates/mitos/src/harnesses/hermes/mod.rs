mod handoffs;
mod streams;
mod turns;

use std::ffi::OsStr;
use std::path::Path;

use anyhow::Result;

use self::handoffs::collect_hermes_handoff;
use self::turns::run_turn;
use crate::harnesses::{Emitter, Harness, Turn};
use crate::history::harness_home;
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{Capabilities, CollectedHandoff};

/// Interactive hermes cannot take an initial prompt and keep its REPL, so the
/// handoff is printed by Mitos and a linked session resumes in the Mitos
/// workspace. Headless turns run over `hermes acp`; no plan mode is offered.
pub struct HermesAdapter;

impl Harness for HermesAdapter {
    const PROGRAM: &'static str = "hermes";

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            headless: true,
            modes: Vec::new(),
            ask_back: true,
        }
    }

    fn launch_args(&self, session: Option<&str>, _context: &str) -> Vec<String> {
        match session {
            Some(session) => vec!["--resume".into(), session.into(), "--no-restore-cwd".into()],
            None => Vec::new(),
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
        let home = harness_home("HERMES_HOME", ".hermes");
        Ok(collect_hermes_handoff(&home, request))
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
            AdapterRequest::prepare_launch(HarnessKind::Hermes, "ctx".into(), native_session);
        HermesAdapter.prepare_launch(&request).unwrap()
    }

    #[test]
    fn resumes_a_linked_session_and_never_takes_the_context_as_an_arg() {
        let fresh = launch(None);
        assert_eq!(fresh.program, "hermes");
        assert_eq!(fresh.args, Vec::<String>::new());
        assert_eq!(fresh.native_session, None);
        let resumed = launch(Some(json!("sess-1")));
        assert_eq!(resumed.args, ["--resume", "sess-1", "--no-restore-cwd"]);
        assert_eq!(resumed.native_session, Some(json!("sess-1")));
    }

    #[test]
    fn is_headless_and_build_only_so_plan_reassignment_is_refused() {
        let capabilities = HermesAdapter.negotiate(HarnessKind::Hermes).unwrap();
        assert!(capabilities.headless);
        assert_eq!(capabilities.modes, []);
        assert!(capabilities.ask_back);
    }
}
