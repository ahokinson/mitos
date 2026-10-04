use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::ThreadService;
use crate::domain::{
    HarnessKind, HarnessRequest, Reply, RequestId, RequestKind, RequestStatus, ThreadId,
    ThreadMode, TurnId,
};
use crate::ipc::answers;
use crate::store::{NewHarnessRequest, StoreError};
use crate::wire::events::AdapterEvent;

impl ThreadService<'_> {
    pub fn pending_requests(&self, thread_id: &ThreadId) -> Result<Vec<HarnessRequest>> {
        self.store.pending_requests(thread_id)
    }

    /// Builds the response the request's adapter expects, then answers with it.
    pub fn reply(
        &self,
        thread_id: &ThreadId,
        request_id: &RequestId,
        reply: &Reply,
    ) -> Result<HarnessRequest> {
        let response = self
            .store
            .get_request(request_id)?
            .kind
            .response_to(reply)?;
        self.answer(thread_id, request_id, &response)
    }

    /// Takes no thread lock: the waiting turn already holds it.
    pub fn answer(
        &self,
        thread_id: &ThreadId,
        request_id: &RequestId,
        response: &Value,
    ) -> Result<HarnessRequest> {
        let request = self.store.get_request(request_id)?;
        if request.thread_id != *thread_id {
            bail!("request {request_id} does not belong to thread {thread_id}");
        }
        if request.status != RequestStatus::Pending {
            return Err(StoreError::RequestNotPending(request_id.clone()).into());
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
        thread_id: &ThreadId,
        harness: HarnessKind,
        turn_id: &TurnId,
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
            thread_id: thread_id.clone(),
            turn_id: Some(turn_id.clone()),
            harness: Some(harness),
            kind,
            payload: Some(payload),
        })?;
        Ok(())
    }
}
