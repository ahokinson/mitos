use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use super::resolvers;
use super::streams::dispatch_ndjson;
use crate::ipc::answers::AnswerListener;
use crate::ports::{EventSink, HarnessAdapter};
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, NegotiateRequest,
    SendMessageRequest, StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan, NegotiateResponse};
use crate::wire::versions::PROTOCOL_VERSION;

fn check_response(protocol_version: u32, kind: &str, expected: &str) -> Result<()> {
    if protocol_version != PROTOCOL_VERSION {
        bail!("adapter protocol version {protocol_version} is unsupported");
    }
    if kind != expected {
        bail!("adapter returned unsupported response kind {kind:?}");
    }
    Ok(())
}

pub struct ExternalAdapter {
    adapter_dir: Option<PathBuf>,
}

impl ExternalAdapter {
    pub fn new(adapter_dir: Option<PathBuf>) -> Self {
        Self { adapter_dir }
    }

    /// Stdin closes after the request unless `keep_stdin`, so EOF-waiting adapters don't hang.
    fn spawn(
        &self,
        harness: &str,
        request: &impl Serialize,
        keep_stdin: bool,
    ) -> Result<(Child, Option<ChildStdin>)> {
        let mut child = resolvers::adapter_command(harness, self.adapter_dir.as_deref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("could not start adapter {harness}"))?;
        let mut stdin = child.stdin.take().context("adapter stdin unavailable")?;
        let mut request_json = serde_json::to_vec(request)?;
        request_json.push(b'\n');
        stdin.write_all(&request_json)?;
        stdin.flush()?;
        Ok((child, keep_stdin.then_some(stdin)))
    }

    fn run_buffered(&self, harness: &str, request: &impl Serialize) -> Result<Vec<u8>> {
        let (child, _) = self.spawn(harness, request, false)?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("adapter {harness} exited with {}", output.status);
        }
        Ok(output.stdout)
    }

    fn run_streaming(
        &self,
        harness: &str,
        request: &impl Serialize,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        let (mut child, stdin) = self.spawn(harness, request, answers.is_some())?;
        let stdout = child.stdout.take().context("adapter stdout unavailable")?;
        let listener = match (answers, stdin) {
            (Some(path), Some(mut stdin)) => Some(AnswerListener::start(path, move |answer| {
                writeln!(stdin, "{answer}")?;
                stdin.flush()?;
                Ok(())
            })?),
            _ => None,
        };
        let native_session = dispatch_ndjson(BufReader::new(stdout), on_event)?;
        drop(listener);
        let status = child.wait()?;
        if !status.success() {
            bail!("adapter {harness} exited with {status}");
        }
        Ok(native_session)
    }
}

impl HarnessAdapter for ExternalAdapter {
    fn negotiate(&self, harness: &str) -> Result<Capabilities> {
        let request = NegotiateRequest::new(harness);
        let output = self.run_buffered(harness, &request)?;
        let response: NegotiateResponse = serde_json::from_slice(&output)
            .with_context(|| format!("adapter {harness} did not return valid negotiate JSON"))?;
        check_response(response.protocol_version, &response.kind, "capabilities")?;
        Ok(response.capabilities)
    }

    fn prepare_launch(&self, request: &AdapterRequest) -> Result<LaunchPlan> {
        let output = self.run_buffered(&request.harness, request)?;
        let launch: LaunchPlan = serde_json::from_slice(&output)
            .with_context(|| format!("adapter {} did not return valid JSON", request.harness))?;
        check_response(launch.protocol_version, &launch.kind, "launch")?;
        if launch.program.is_empty() {
            bail!("adapter returned an empty program");
        }
        Ok(launch)
    }

    fn collect_handoff(&self, request: &HandoffRequest) -> Result<CollectedHandoff> {
        let output = self.run_buffered(&request.harness, request)?;
        let handoff: CollectedHandoff = serde_json::from_slice(&output).with_context(|| {
            format!(
                "adapter {} did not return valid handoff JSON",
                request.harness
            )
        })?;
        check_response(handoff.protocol_version, &handoff.kind, "handoff")?;
        Ok(handoff)
    }

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        self.run_streaming(&request.harness, request, on_event, answers)
    }

    fn attach_thread(
        &self,
        request: &AttachThreadRequest,
        on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        self.run_streaming(&request.harness, request, on_event, None)?;
        Ok(())
    }

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        answers: Option<&Path>,
    ) -> Result<()> {
        self.run_streaming(&request.harness, request, on_event, answers)?;
        Ok(())
    }

    fn detach_thread(&self, request: &DetachThreadRequest) -> Result<()> {
        self.run_buffered(&request.harness, request)?;
        Ok(())
    }
}
