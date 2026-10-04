use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

use super::processes::ExternalAdapter;
use crate::harnesses::{ClaudeAdapter, CodexAdapter, HermesAdapter, OpenCodeAdapter};
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

/// Native adapters serve their harness unless `--adapter-dir` points at
/// external ones; every other harness goes through an external adapter.
pub struct Adapters {
    external: ExternalAdapter,
    native: bool,
}

impl Adapters {
    pub fn new(adapter_dir: Option<PathBuf>) -> Self {
        Self {
            native: adapter_dir.is_none(),
            external: ExternalAdapter::new(adapter_dir),
        }
    }

    fn route(&self, harness: &str) -> &dyn HarnessAdapter {
        match harness {
            "claude" if self.native => &ClaudeAdapter,
            "codex" if self.native => &CodexAdapter,
            "hermes" if self.native => &HermesAdapter,
            "opencode" if self.native => &OpenCodeAdapter,
            _ => &self.external,
        }
    }
}

impl HarnessAdapter for Adapters {
    fn negotiate(&self, harness: &str) -> Result<Capabilities> {
        self.route(harness).negotiate(harness)
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        self.route(&request.harness).prepare_launch(request)
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        self.route(&request.harness).collect_handoff(request)
    }

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        self.route(&request.harness)
            .start_thread(request, on_event, answers)
    }

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        self.route(&request.harness)
            .attach_thread(request, on_event)
    }

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<()> {
        self.route(&request.harness)
            .send_message(request, on_event, answers)
    }

    fn detach_thread(&self, request: &DetachThreadRequest) -> Result<()> {
        self.route(&request.harness).detach_thread(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_is_native_unless_an_adapter_dir_is_given() {
        let native = Adapters::new(None);
        let capabilities = native.negotiate("claude").unwrap();
        assert!(capabilities.headless);
        assert!(!capabilities.ask_back);

        let overridden = Adapters::new(Some(PathBuf::from("/nonexistent")));
        assert!(overridden.negotiate("claude").is_err());
    }
}
