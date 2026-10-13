use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use super::{ClaudeAdapter, CodexAdapter, HermesAdapter, OpenCodeAdapter};
use crate::domain::HarnessKind;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, MiningRequest,
    SendMessageRequest, StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan, MiningReport};

pub struct Adapters;

impl Adapters {
    fn route(harness: HarnessKind) -> &'static dyn HarnessAdapter {
        match harness {
            HarnessKind::Claude => &ClaudeAdapter,
            HarnessKind::Codex => &CodexAdapter,
            HarnessKind::Hermes => &HermesAdapter,
            HarnessKind::OpenCode => &OpenCodeAdapter,
        }
    }

    /// One harness's mined history.
    pub fn mine_one(harness: HarnessKind, request: &MiningRequest) -> MiningReport {
        Self::route(harness).mine_history(request)
    }

    /// Every harness's mined history, folded into one report; a harness
    /// without a minable history contributes nothing.
    pub fn mine_all(request: &MiningRequest) -> MiningReport {
        HarnessKind::ALL
            .into_iter()
            .map(|harness| Self::route(harness).mine_history(request))
            .fold(MiningReport::default(), |merged, report| {
                merged.merged(report)
            })
    }
}

impl HarnessAdapter for Adapters {
    fn negotiate(&self, harness: HarnessKind) -> Result<Capabilities> {
        Self::route(harness).negotiate(harness)
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        Self::route(request.harness).prepare_launch(request)
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        Self::route(request.harness).collect_handoff(request)
    }

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        Self::route(request.harness).start_thread(request, on_event, answers)
    }

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        Self::route(request.harness).attach_thread(request, on_event)
    }

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<()> {
        Self::route(request.harness).send_message(request, on_event, answers)
    }

    fn detach_thread(&self, request: &DetachThreadRequest) -> Result<()> {
        Self::route(request.harness).detach_thread(request)
    }

    fn mine_history(&self, request: &MiningRequest) -> MiningReport {
        Self::mine_all(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_routes_to_its_own_adapter() {
        for harness in HarnessKind::ALL {
            assert!(Adapters.negotiate(harness).unwrap().headless);
        }
        assert!(!Adapters.negotiate(HarnessKind::Claude).unwrap().ask_back);
    }

    #[test]
    fn a_harness_without_minable_history_reports_nothing() {
        let request = crate::wire::requests::MiningRequest::new(None, None, 10);
        assert_eq!(
            Adapters::mine_one(HarnessKind::Hermes, &request),
            MiningReport::default()
        );
    }
}
