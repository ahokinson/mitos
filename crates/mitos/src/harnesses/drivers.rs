use std::path::Path;

use anyhow::{Result, bail};
use serde_json::Value;

use super::{Emitter, Turn};
use crate::domain::HarnessKind;
use crate::json::string_value;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, MiningRequest,
    SendMessageRequest, StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan, MiningReport};

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

    /// Harnesses with a stored history miners can read override this; the
    /// default reports none.
    fn mine(&self, _request: &MiningRequest) -> MiningReport {
        MiningReport::default()
    }

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

    fn mine_history(&self, request: &MiningRequest) -> MiningReport {
        self.mine(request)
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
            agent: request.agent.as_deref(),
            model: request.model.as_deref(),
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
            agent: request.agent.as_deref(),
            model: request.model.as_deref(),
        };
        Emitter::collect(on_event, |emitter| self.run_turn(&turn, emitter, answers))?;
        Ok(())
    }

    fn detach_thread(&self, _request: &DetachThreadRequest) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::domain::ThreadMode;

    #[derive(Default)]
    struct FakeHarness {
        turns: RefCell<Vec<(Option<String>, String, bool)>>,
    }

    impl Harness for FakeHarness {
        const PROGRAM: &'static str = "fake";

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                headless: true,
                modes: vec![ThreadMode::Build, ThreadMode::Plan],
                ask_back: true,
            }
        }

        fn launch_args(&self, session: Option<&str>, context: &str) -> Vec<String> {
            [session.unwrap_or("new"), context]
                .into_iter()
                .map(str::to_string)
                .collect()
        }

        fn run_turn(
            &self,
            turn: &Turn<'_>,
            emitter: &mut Emitter<'_, '_>,
            _answers: Option<&Path>,
        ) -> Result<()> {
            self.turns.borrow_mut().push((
                turn.session.map(str::to_string),
                turn.text.to_string(),
                turn.ephemeral,
            ));
            emitter.emit(AdapterEvent {
                native_session: Some(json!("native-session")),
                ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
            })?;
            emitter.emit(AdapterEvent::new(kinds::TURN_COMPLETE))
        }

        fn collect(&self, _request: &HandoffRequest) -> Result<CollectedHandoff> {
            Ok(CollectedHandoff::new(Some(json!("collected")), None, None))
        }
    }

    fn workdir() -> PathBuf {
        PathBuf::from("/workspace")
    }

    #[test]
    fn prepares_launches_and_collects_handoffs() {
        let harness = FakeHarness::default();
        let request = AdapterRequest::prepare_launch(
            HarnessKind::Claude,
            "context".into(),
            Some(json!("existing")),
        );

        assert!(harness.negotiate(HarnessKind::Claude).unwrap().ask_back);
        let launch = harness.prepare_launch(&request).unwrap();
        assert_eq!(launch.program, "fake");
        assert_eq!(launch.args, ["existing", "context"]);
        assert_eq!(launch.native_session, Some(json!("existing")));
        assert_eq!(
            harness
                .collect_handoff(&HandoffRequest::collect(
                    HarnessKind::Claude,
                    &workdir(),
                    "2026-01-01T00:00:00Z".into(),
                    None,
                ))
                .unwrap()
                .native_session,
            Some(json!("collected"))
        );
        assert_eq!(
            harness.mine_history(&MiningRequest::new(None, None, 1)),
            MiningReport::default()
        );
    }

    #[test]
    fn starts_sends_attaches_and_detaches_threads() {
        let harness = FakeHarness::default();
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let start = StartThreadRequest::new(
            HarnessKind::Claude,
            workdir(),
            ThreadMode::Plan,
            "start".into(),
        )
        .ephemeral();

        assert_eq!(
            harness.start_thread(&start, &mut sink, None).unwrap(),
            Some(json!("native-session"))
        );
        let missing = SendMessageRequest::new(
            HarnessKind::Claude,
            workdir(),
            None,
            ThreadMode::Build,
            "send".into(),
        );
        assert_eq!(
            harness
                .send_message(&missing, &mut sink, None)
                .unwrap_err()
                .to_string(),
            "send_message requires a native_session"
        );
        let send = SendMessageRequest::new(
            HarnessKind::Claude,
            workdir(),
            Some(json!("existing")),
            ThreadMode::Build,
            "send".into(),
        );
        harness.send_message(&send, &mut sink, None).unwrap();
        harness
            .attach_thread(
                &AttachThreadRequest::new(HarnessKind::Claude, workdir(), None),
                &mut sink,
            )
            .unwrap();
        harness
            .detach_thread(&DetachThreadRequest::new(HarnessKind::Claude))
            .unwrap();

        assert_eq!(
            harness.turns.into_inner(),
            [
                (None, "start".into(), true),
                (Some("existing".into()), "send".into(), false)
            ]
        );
        assert!(
            events
                .iter()
                .any(|event| event.event == kinds::TURN_COMPLETE)
        );
    }
}
