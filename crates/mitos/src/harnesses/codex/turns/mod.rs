mod process;

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::requests::{PendingServerRequest, request_from_server, result_for};
use super::streams::{process_notification, process_turn_completed};
use super::tokens::TurnTokens;
use crate::domain::ThreadMode;
use crate::harnesses::{Emitter, Turn};
use crate::rpc::{Inbound, RpcPeer, RpcWriter};
use crate::wire::events::{AdapterEvent, kinds};

pub use process::run_turn;

const METHOD_NOT_FOUND: i64 = -32601;
const NO_THREAD_ID: &str = "codex did not return a thread id";
const NOT_COMPLETED: &str = "codex ended without completing the turn";

type SharedPending = Arc<Mutex<HashMap<String, PendingServerRequest>>>;

/// Codex never asks for approval; plan is read-only, build is unrestricted.
fn sandbox_policy(mode: ThreadMode) -> Value {
    match mode {
        ThreadMode::Plan => json!({ "type": "readOnly" }),
        ThreadMode::Build => json!({ "type": "dangerFullAccess" }),
    }
}

/// The thread-level `sandbox` value matching `sandbox_policy`.
fn thread_sandbox(mode: ThreadMode) -> &'static str {
    match mode {
        ThreadMode::Plan => "read-only",
        ThreadMode::Build => "danger-full-access",
    }
}

struct Session<'a, 'b, 'c> {
    emitter: &'a mut Emitter<'b, 'c>,
    writer: RpcWriter,
    pending: SharedPending,
    tokens: TurnTokens,
    finished: &'a Cell<bool>,
}

impl Session<'_, '_, '_> {
    fn handle(&mut self, inbound: Inbound) -> Result<()> {
        match inbound {
            Inbound::Notification { method, params } => {
                if method == "turn/completed" {
                    self.emitter.emit(process_turn_completed(&params))?;
                    self.finished.set(true);
                    return Ok(());
                }
                for event in process_notification(&method, &params, &mut self.tokens) {
                    self.emitter.emit(event)?;
                }
            }
            Inbound::Request { id, method, params } => {
                match request_from_server(&id, &method, &params) {
                    None => self.writer.respond_error(
                        &id,
                        METHOD_NOT_FOUND,
                        &format!("Mitos cannot answer {method}"),
                    )?,
                    Some((event, pending)) => {
                        self.pending
                            .lock()
                            .expect("codex pending lock poisoned")
                            .insert(id.to_text(), pending);
                        self.emitter.emit(event)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn answer_handler(
    writer: RpcWriter,
    pending: SharedPending,
) -> impl FnMut(Value) -> Result<()> + Send + 'static {
    move |answer| {
        if answer.get("action").and_then(Value::as_str) != Some("answer") {
            return Ok(());
        }
        let Some(request_id) = answer.get("request_id").and_then(Value::as_str) else {
            return Ok(());
        };
        let request = pending
            .lock()
            .expect("codex pending lock poisoned")
            .remove(request_id);
        let Some(request) = request else {
            return Ok(());
        };
        let response = answer.get("response").unwrap_or(&Value::Null);
        writer.respond(&request.rpc_id, result_for(&request, response))
    }
}

fn drive(turn: &Turn<'_>, peer: &mut RpcPeer, session: &mut Session<'_, '_, '_>) -> Result<()> {
    let workdir = turn.workdir.to_string_lossy();
    peer.call(
        "initialize",
        Some(json!({ "clientInfo": { "name": "mitos", "title": "Mitos", "version": "0.1.0" } })),
        &mut |inbound| session.handle(inbound),
    )?;
    session.writer.notify("initialized", None)?;

    let mut thread_params = Map::new();
    if let Some(existing) = turn.session {
        thread_params.insert("threadId".into(), json!(existing));
        thread_params.insert("excludeTurns".into(), json!(true));
    }
    thread_params.insert("cwd".into(), json!(workdir));
    thread_params.insert("approvalPolicy".into(), json!("never"));
    thread_params.insert("sandbox".into(), json!(thread_sandbox(turn.mode)));
    if turn.ephemeral {
        thread_params.insert("ephemeral".into(), json!(true));
    }
    let method = if turn.session.is_some() {
        "thread/resume"
    } else {
        "thread/start"
    };
    let started = peer.call(method, Some(Value::Object(thread_params)), &mut |inbound| {
        session.handle(inbound)
    })?;
    let Some(thread_id) = started.pointer("/thread/id").and_then(Value::as_str) else {
        bail!(NO_THREAD_ID);
    };
    session.emitter.emit(AdapterEvent {
        native_session: Some(json!(thread_id)),
        ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
    })?;

    session.tokens.begin();
    peer.call(
        "turn/start",
        Some(json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": turn.text }],
            "cwd": workdir,
            "approvalPolicy": "never",
            "sandboxPolicy": sandbox_policy(turn.mode),
        })),
        &mut |inbound| session.handle(inbound),
    )?;
    let finished = session.finished;
    peer.pump(&mut |inbound| session.handle(inbound), &mut || {
        finished.get()
    })
}

fn error_event(content: String) -> AdapterEvent {
    AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn plan_is_read_only_and_build_is_unrestricted() {
        assert_eq!(
            sandbox_policy(ThreadMode::Plan),
            json!({ "type": "readOnly" })
        );
        assert_eq!(
            sandbox_policy(ThreadMode::Build),
            json!({ "type": "dangerFullAccess" })
        );
        assert_eq!(thread_sandbox(ThreadMode::Plan), "read-only");
        assert_eq!(thread_sandbox(ThreadMode::Build), "danger-full-access");
    }
}
