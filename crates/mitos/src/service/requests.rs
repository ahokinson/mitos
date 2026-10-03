use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::ThreadService;
use crate::domain::{HarnessRequest, RequestKind, RequestStatus, ThreadMode};
use crate::ipc::answers;
use crate::store::NewHarnessRequest;
use crate::wire::events::AdapterEvent;

impl ThreadService<'_> {
    pub fn pending_requests(&self, thread_id: &str) -> Result<Vec<HarnessRequest>> {
        self.store.pending_requests(thread_id)
    }

    /// Takes no thread lock: the waiting turn already holds it.
    pub fn answer(
        &self,
        thread_id: &str,
        request_id: &str,
        response: &Value,
    ) -> Result<HarnessRequest> {
        let request = self.store.get_request(request_id)?;
        if request.thread_id != thread_id {
            bail!("request {request_id} does not belong to thread {thread_id}");
        }
        if request.status != RequestStatus::Pending {
            bail!("request {request_id} is not pending");
        }
        let native_id = request
            .payload
            .as_ref()
            .and_then(|payload| payload.get("id"))
            .cloned()
            .context("request has no adapter id")?;
        let line = serde_json::json!({
            "action": "answer",
            "request_id": native_id,
            "response": response,
        });
        let socket = answers::socket_path(self.store.config_root(), thread_id);
        if let Err(error) = answers::send_answer(&socket, &line) {
            self.store.cancel_pending_requests(thread_id)?;
            return Err(
                error.context("the turn that asked is no longer running; request cancelled")
            );
        }
        let answered = self.store.answer_request(request_id, response)?;
        if answered.kind == RequestKind::PlanApproval
            && response.get("approved").and_then(Value::as_bool) == Some(true)
        {
            self.set_mode(thread_id, ThreadMode::Build)?;
        }
        Ok(answered)
    }

    pub(super) fn open_adapter_request(
        &self,
        thread_id: &str,
        harness: &str,
        turn_id: &str,
        event: AdapterEvent,
    ) -> Result<()> {
        let payload = event.payload.context("request event has no payload")?;
        payload
            .get("id")
            .and_then(Value::as_str)
            .context("request event payload has no id")?;
        let kind = payload
            .get("kind")
            .and_then(Value::as_str)
            .context("request event payload has no kind")?
            .parse()?;
        self.store.open_request(NewHarnessRequest {
            thread_id: thread_id.into(),
            turn_id: Some(turn_id.into()),
            harness: Some(harness.into()),
            kind,
            payload: Some(payload),
        })?;
        Ok(())
    }
}
