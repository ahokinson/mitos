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

fn finish_turn(
    outcome: Result<()>,
    complete: bool,
    diagnostic: String,
    emitter: &mut Emitter<'_, '_>,
) -> Result<()> {
    match outcome {
        Ok(()) if complete => Ok(()),
        Ok(()) => emitter.emit(error_event(if diagnostic.is_empty() {
            NOT_COMPLETED.into()
        } else {
            diagnostic
        })),
        Err(cause) => emitter.emit(error_event(if diagnostic.is_empty() {
            cause.to_string()
        } else {
            diagnostic
        })),
    }
}

fn error_event(content: String) -> AdapterEvent {
    AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::{Cursor, Write};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::harnesses::fixtures::kinds_of;
    use serde_json::{Value, json};

    const THREAD: &str = "th-1";

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn messages(&self) -> Vec<Value> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
    }

    fn reply(id: i64, result: impl serde::Serialize) -> Value {
        json!({ "id": id, "result": result })
    }

    fn peer(messages: &[Value]) -> (RpcPeer, Captured) {
        let input = messages.iter().fold(String::new(), |mut input, message| {
            input.push_str(&message.to_string());
            input.push('\n');
            input
        });
        let captured = Captured::default();
        (
            RpcPeer::new(captured.clone(), Cursor::new(input), false),
            captured,
        )
    }

    fn script(thread: impl Into<Value>, tail: impl IntoIterator<Item = Value>) -> Vec<Value> {
        let thread = thread.into();
        let mut messages = vec![
            reply(1, json!({})),
            reply(2, json!({ "thread": { "id": thread } })),
            reply(3, json!({})),
        ];
        messages.extend(tail);
        messages
    }

    fn run(
        messages: &[Value],
        session: Option<&str>,
        mode: ThreadMode,
        ephemeral: bool,
        diagnostic: &str,
    ) -> (Vec<AdapterEvent>, Option<Value>, Vec<Value>) {
        let (mut peer, captured) = peer(messages);
        let mut seen = Vec::new();
        let native_session = {
            let mut sink = |event: AdapterEvent| {
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let finished = Cell::new(false);
            let mut protocol = Session {
                emitter: &mut emitter,
                writer: peer.writer(),
                pending: SharedPending::default(),
                tokens: TurnTokens::default(),
                finished: &finished,
            };
            let turn = Turn {
                workdir: Path::new("/work"),
                text: "hello",
                session,
                mode,
                ephemeral,
                agent: None,
                model: None,
            };
            let outcome = drive(&turn, &mut peer, &mut protocol);
            let complete = finished.get();
            drop(protocol);
            finish_turn(outcome, complete, diagnostic.into(), &mut emitter).unwrap();
            emitter.into_native_session()
        };
        (seen, native_session, captured.messages())
    }

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

    #[test]
    fn a_turn_streams_notifications_and_ends_at_turn_completed() {
        let (events, native, _) = run(
            &script(
                json!(THREAD),
                [
                    json!({ "method": "item/agentMessage/delta", "params": { "delta": "Hi" } }),
                    json!({ "method": "item/completed", "params": { "item": { "type": "agentMessage", "text": "Done." } } }),
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "completed" } } }),
                ],
            ),
            None,
            ThreadMode::Build,
            false,
            "",
        );
        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_DELTA,
                kinds::ASSISTANT_MESSAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(native, Some(json!(THREAD)));
    }

    #[test]
    fn requests_carry_the_session_mode_and_prompt() {
        let (_, native, sent) = run(
            &script(
                json!("th-9"),
                [
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "completed" } } }),
                ],
            ),
            Some("th-9"),
            ThreadMode::Build,
            false,
            "",
        );
        assert_eq!(native, Some(json!("th-9")));
        let thread = sent
            .iter()
            .find(|message| message["method"] == "thread/resume")
            .unwrap();
        assert_eq!(thread["params"]["threadId"], "th-9");
        assert_eq!(thread["params"]["excludeTurns"], true);
        assert_eq!(thread["params"]["sandbox"], "danger-full-access");
        assert_eq!(thread["params"]["approvalPolicy"], "never");
        let turn = sent
            .iter()
            .find(|message| message["method"] == "turn/start")
            .unwrap();
        assert_eq!(
            turn["params"]["input"],
            json!([{ "type": "text", "text": "hello" }])
        );
        assert_eq!(
            turn["params"]["sandboxPolicy"],
            json!({ "type": "dangerFullAccess" })
        );
    }

    #[test]
    fn an_ephemeral_turn_starts_an_ephemeral_read_only_thread() {
        let (_, _, sent) = run(
            &script(
                json!("th-e"),
                [
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "completed" } } }),
                ],
            ),
            None,
            ThreadMode::Plan,
            true,
            "",
        );
        let thread = sent
            .iter()
            .find(|message| message["method"] == "thread/start")
            .unwrap();
        assert_eq!(thread["params"]["ephemeral"], true);
        assert_eq!(thread["params"]["sandbox"], "read-only");
    }

    #[test]
    fn unanswerable_server_requests_are_rejected() {
        let (events, _, sent) = run(
            &script(
                json!(THREAD),
                [
                    json!({ "id": "s1", "method": "attestation/generate", "params": {} }),
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "completed" } } }),
                ],
            ),
            None,
            ThreadMode::Build,
            false,
            "",
        );
        assert_eq!(
            kinds_of(&events),
            [kinds::NATIVE_SESSION_UPDATE, kinds::TURN_COMPLETE]
        );
        let rejection = sent.iter().find(|message| message["id"] == "s1").unwrap();
        assert_eq!(rejection["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn a_server_request_becomes_a_request_event() {
        let (events, _, _) = run(
            &script(
                json!(THREAD),
                [
                    json!({ "id": 5, "method": "item/commandExecution/requestApproval", "params": { "command": "ls" } }),
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "completed" } } }),
                ],
            ),
            None,
            ThreadMode::Build,
            false,
            "",
        );
        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::REQUEST,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(events[1].payload.as_ref().unwrap()["id"], "5");
    }

    #[test]
    fn an_unfinished_turn_reports_a_diagnostic_or_default() {
        let messages = script(json!(THREAD), []);
        let (diagnosed, _, _) = run(&messages, None, ThreadMode::Build, false, "boom");
        assert_eq!(
            kinds_of(&diagnosed),
            [kinds::NATIVE_SESSION_UPDATE, kinds::ERROR]
        );
        assert_eq!(diagnosed[1].content.as_deref(), Some("boom"));

        let (silent, _, _) = run(&messages, None, ThreadMode::Build, false, "");
        assert_eq!(
            silent.last().unwrap().content.as_deref(),
            Some(NOT_COMPLETED)
        );
    }

    #[test]
    fn a_missing_thread_id_is_an_error_event() {
        let (events, _, _) = run(&script(Value::Null, []), None, ThreadMode::Build, false, "");
        assert_eq!(kinds_of(&events), [kinds::ERROR]);
        assert_eq!(events[0].content.as_deref(), Some(NO_THREAD_ID));
    }

    #[test]
    fn a_failed_turn_surfaces_the_servers_error() {
        let (events, _, _) = run(
            &script(
                json!(THREAD),
                [
                    json!({ "method": "turn/completed", "params": { "turn": { "status": "failed", "error": { "message": "nope" } } } }),
                ],
            ),
            None,
            ThreadMode::Build,
            false,
            "",
        );
        assert_eq!(
            kinds_of(&events),
            [kinds::NATIVE_SESSION_UPDATE, kinds::ERROR]
        );
        assert_eq!(events[1].content.as_deref(), Some("nope"));
    }
}
