use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::domain::HarnessKind;
use crate::wire::events::AdapterEvent;
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

pub type EventSink<'a> = dyn FnMut(AdapterEvent) -> Result<()> + 'a;

pub trait HarnessAdapter {
    fn negotiate(&self, harness: HarnessKind) -> Result<Capabilities>;

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan>;
    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff>;

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>>;

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()>;

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<()>;

    /// Must default to leaving the native session running.
    fn detach_thread(&self, request: &DetachThreadRequest) -> Result<()>;
}
