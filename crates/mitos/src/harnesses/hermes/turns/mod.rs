mod process;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::streams::{
    AcpStream, PendingPermission, REQUEST_PERMISSION, SESSION_UPDATE, outcome_for,
    permission_request,
};
use crate::harnesses::{Emitter, Turn};
use crate::json::string_value;
use crate::rpc::{Inbound, PeerExited, RpcPeer, RpcWriter};
use crate::wire::events::{AdapterEvent, kinds};

pub use process::run_turn;

const PROTOCOL_VERSION: u32 = 1;
const METHOD_NOT_FOUND: i64 = -32601;
const EXITED: &str = "hermes acp exited";

pub type SharedPending = Arc<Mutex<HashMap<String, PendingPermission>>>;

struct Session<'a, 'b, 'c> {
    emitter: &'a mut Emitter<'b, 'c>,
    writer: RpcWriter,
    pending: SharedPending,
    stream: Option<AcpStream>,
}

impl Session<'_, '_, '_> {
    /// Updates that arrive before the session is ready are history replayed by
    /// `session/load`, not part of this turn.
    fn handle(&mut self, inbound: Inbound) -> Result<()> {
        match inbound {
            Inbound::Notification { method, params } => {
                if method != SESSION_UPDATE {
                    return Ok(());
                }
                if let Some(stream) = self.stream.as_mut() {
                    for event in stream.update(&params) {
                        self.emitter.emit(event)?;
                    }
                }
            }
            Inbound::Request { id, method, params } => {
                let request = (method == REQUEST_PERMISSION && self.stream.is_some())
                    .then(|| permission_request(&id, &params))
                    .flatten();
                match request {
                    None => self.writer.respond_error(
                        &id,
                        METHOD_NOT_FOUND,
                        &format!("Mitos cannot answer {method}"),
                    )?,
                    Some((event, pending)) => {
                        self.pending
                            .lock()
                            .expect("hermes pending lock poisoned")
                            .insert(id.to_text(), pending);
                        self.emitter.emit(event)?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn answer_handler(
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
            .expect("hermes pending lock poisoned")
            .remove(request_id);
        let Some(request) = request else {
            return Ok(());
        };
        let response = answer.get("response").unwrap_or(&Value::Null);
        writer.respond(&request.rpc_id, outcome_for(&request, response))
    }
}

struct AgentCapabilities {
    load_session: bool,
    resume_session: bool,
}

fn capabilities_of(result: &Value) -> AgentCapabilities {
    let agent = result.get("agentCapabilities");
    AgentCapabilities {
        load_session: agent.and_then(|agent| agent.get("loadSession")) == Some(&Value::Bool(true)),
        resume_session: agent
            .and_then(|agent| agent.get("sessionCapabilities"))
            .and_then(Value::as_object)
            .is_some_and(|capabilities| capabilities.contains_key("resume")),
    }
}

fn open_session(
    turn: &Turn<'_>,
    peer: &mut RpcPeer,
    session: &mut Session<'_, '_, '_>,
    capabilities: &AgentCapabilities,
) -> Result<String> {
    let cwd = turn.workdir.to_string_lossy();
    let Some(existing) = turn.session else {
        let created = peer.call(
            "session/new",
            Some(json!({ "cwd": cwd, "mcpServers": [] })),
            &mut |inbound| session.handle(inbound),
        )?;
        let Some(id) = string_value(created.get("sessionId")) else {
            bail!("hermes did not return a session id");
        };
        return Ok(id.to_string());
    };
    let method = if capabilities.resume_session {
        "session/resume"
    } else if capabilities.load_session {
        "session/load"
    } else {
        bail!("hermes cannot resume sessions");
    };
    peer.call(
        method,
        Some(json!({ "cwd": cwd, "mcpServers": [], "sessionId": existing })),
        &mut |inbound| session.handle(inbound),
    )?;
    Ok(existing.to_string())
}

fn run(turn: &Turn<'_>, peer: &mut RpcPeer, session: &mut Session<'_, '_, '_>) -> Result<()> {
    let initialized = peer.call(
        "initialize",
        Some(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {},
            "clientInfo": { "name": "mitos", "title": "Mitos", "version": "0.1.0" },
        })),
        &mut |inbound| session.handle(inbound),
    )?;
    let capabilities = capabilities_of(&initialized);
    let session_id = open_session(turn, peer, session, &capabilities)?;
    session.emitter.emit(AdapterEvent {
        native_session: Some(json!(session_id)),
        ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
    })?;
    session.stream = Some(AcpStream::new(&session_id));

    let result = peer.call(
        "session/prompt",
        Some(json!({
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": turn.text }],
        })),
        &mut |inbound| session.handle(inbound),
    )?;
    let events = session
        .stream
        .as_mut()
        .map(|stream| stream.finish(&result))
        .unwrap_or_default();
    for event in events {
        session.emitter.emit(event)?;
    }
    Ok(())
}

/// Drives one turn over an open ACP link: the caller owns the child process.
/// Emits exactly one terminal event; `diagnostic` says why the process died
/// when it left no better message.
pub fn drive_turn(
    turn: &Turn<'_>,
    peer: &mut RpcPeer,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
    diagnostic: &dyn Fn() -> String,
) -> Result<()> {
    let mut session = Session {
        emitter,
        writer: peer.writer(),
        pending: pending.clone(),
        stream: None,
    };
    let Err(cause) = run(turn, peer, &mut session) else {
        return Ok(());
    };
    let content = if cause.downcast_ref::<PeerExited>().is_some() {
        let detail = diagnostic();
        if detail.is_empty() {
            EXITED.to_string()
        } else {
            format!("{EXITED}: {detail}")
        }
    } else {
        cause.to_string()
    };
    session.emitter.emit(AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::domain::ThreadMode;
    use crate::wire::events::AdapterEvent;

    const SESSION: &str = "sess-1";

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

    fn kinds_of(events: &[AdapterEvent]) -> Vec<&str> {
        events.iter().map(|event| event.event.as_str()).collect()
    }

    fn peer(script: Vec<Value>) -> (RpcPeer, Captured) {
        let input = script
            .into_iter()
            .map(|message| serde_json::to_string(&message).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        let captured = Captured::default();
        (
            RpcPeer::new(captured.clone(), Cursor::new(input), true),
            captured,
        )
    }

    fn result(id: i64, value: impl serde::Serialize) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "result": value })
    }

    fn methods(captured: &Captured) -> Vec<String> {
        captured
            .messages()
            .iter()
            .filter_map(|message| message["method"].as_str().map(str::to_string))
            .collect()
    }

    fn notification(update: impl serde::Serialize) -> Value {
        json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": SESSION, "update": update } })
    }

    fn capabilities() -> Value {
        json!({ "protocolVersion": 1, "agentCapabilities": { "loadSession": true, "sessionCapabilities": { "resume": {} } } })
    }

    fn standard_turn() -> Vec<Value> {
        vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
            notification(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Hi there" }
            })),
            result(3, json!({ "stopReason": "end_turn" })),
        ]
    }

    fn drive(
        peer: &mut RpcPeer,
        session: Option<&str>,
        pending: &SharedPending,
        diagnostic: &dyn Fn() -> String,
        on_event: &mut dyn FnMut(&AdapterEvent) -> Result<()>,
    ) -> Vec<AdapterEvent> {
        let mut seen = Vec::new();
        {
            let mut sink = |event: AdapterEvent| {
                on_event(&event)?;
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let turn = Turn {
                workdir: Path::new("/work"),
                text: "hello",
                session,
                mode: ThreadMode::Build,
                ephemeral: false,
                agent: None,
                model: None,
            };
            drive_turn(&turn, peer, &mut emitter, pending, diagnostic).unwrap();
        }
        seen
    }

    fn run_plain(peer: &mut RpcPeer, session: Option<&str>) -> Vec<AdapterEvent> {
        let pending = SharedPending::default();
        drive(peer, session, &pending, &String::new, &mut |_| Ok(()))
    }

    #[test]
    fn a_new_session_streams_a_turn_and_ends_with_one_turn_complete() {
        let (mut peer, captured) = peer(standard_turn());
        let events = run_plain(&mut peer, None);
        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_DELTA,
                kinds::ASSISTANT_MESSAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(events[0].native_session, Some(json!(SESSION)));
        let sent = captured.messages();
        assert_eq!(sent[0]["jsonrpc"], "2.0");
        assert_eq!(sent[0]["method"], "initialize");
        assert_eq!(sent[0]["params"]["protocolVersion"], 1);
        assert_eq!(sent[1]["method"], "session/new");
        assert_eq!(
            sent[1]["params"],
            json!({ "cwd": "/work", "mcpServers": [] })
        );
        assert_eq!(sent[2]["method"], "session/prompt");
        assert_eq!(
            sent[2]["params"],
            json!({ "sessionId": SESSION, "prompt": [{ "type": "text", "text": "hello" }] })
        );
    }

    #[test]
    fn an_existing_session_is_resumed_without_creating_one() {
        let (mut peer, captured) = peer(vec![
            result(1, capabilities()),
            result(2, Value::Null),
            notification(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Hi there" }
            })),
            result(3, json!({ "stopReason": "end_turn" })),
        ]);
        let events = run_plain(&mut peer, Some(SESSION));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let methods = methods(&captured);
        assert!(methods.contains(&"session/resume".to_string()));
        assert!(!methods.contains(&"session/new".to_string()));
        assert!(!methods.contains(&"session/load".to_string()));
    }

    #[test]
    fn load_replay_is_not_part_of_the_turn_when_only_session_load_exists() {
        let (mut peer, captured) = peer(vec![
            result(
                1,
                json!({ "protocolVersion": 1, "agentCapabilities": { "loadSession": true } }),
            ),
            notification(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "old history" }
            })),
            result(2, Value::Null),
            notification(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Hi there" }
            })),
            result(3, json!({ "stopReason": "end_turn" })),
        ]);
        let events = run_plain(&mut peer, Some(SESSION));
        assert!(methods(&captured).contains(&"session/load".to_string()));
        assert!(
            events
                .iter()
                .all(|event| event.content.as_deref() != Some("old history"))
        );
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
    }

    #[test]
    fn an_agent_that_cannot_resume_is_reported_as_an_error() {
        let (mut peer, _) = peer(vec![result(
            1,
            json!({ "protocolVersion": 1, "agentCapabilities": {} }),
        )]);
        let events = run_plain(&mut peer, Some(SESSION));
        assert_eq!(kinds_of(&events), [kinds::ERROR]);
        assert_eq!(
            events[0].content.as_deref(),
            Some("hermes cannot resume sessions")
        );
    }

    #[test]
    fn a_permission_request_is_answered_with_the_chosen_option() {
        let (mut peer, captured) = peer(vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "session/request_permission",
                "params": {
                    "sessionId": SESSION,
                    "toolCall": { "toolCallId": "t1", "title": "Run ls" },
                    "options": [
                        { "optionId": "ao", "name": "Allow", "kind": "allow_once" },
                        { "optionId": "ro", "name": "Reject", "kind": "reject_once" }
                    ]
                }
            }),
            result(3, json!({ "stopReason": "end_turn" })),
        ]);
        let pending = SharedPending::default();
        let mut answer = answer_handler(peer.writer(), pending.clone());
        let events = drive(&mut peer, None, &pending, &String::new, &mut |event| {
            if event.event == kinds::REQUEST {
                answer(json!({
                    "action": "answer",
                    "request_id": "9",
                    "response": { "allow": true }
                }))?;
            }
            Ok(())
        });
        assert!(kinds_of(&events).contains(&kinds::REQUEST));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let reply = captured
            .messages()
            .into_iter()
            .find(|message| message["id"] == 9 && message.get("result").is_some())
            .unwrap();
        assert_eq!(
            reply["result"],
            json!({ "outcome": { "outcome": "selected", "optionId": "ao" } })
        );
    }

    #[test]
    fn an_unknown_agent_request_is_rejected() {
        let (mut peer, captured) = peer(vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "fs/read_text_file",
                "params": { "sessionId": SESSION, "path": "/etc/passwd" }
            }),
            result(3, json!({ "stopReason": "end_turn" })),
        ]);
        run_plain(&mut peer, None);
        let rejection = captured
            .messages()
            .into_iter()
            .find(|message| message["id"] == 5 && message.get("error").is_some())
            .unwrap();
        assert_eq!(rejection["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn an_rpc_error_is_the_single_terminal_error_with_the_servers_details() {
        let (mut peer, _) = peer(vec![
            result(1, capabilities()),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "error": {
                    "code": -32603,
                    "message": "Internal error",
                    "data": { "details": "No LLM provider configured" }
                }
            }),
        ]);
        let events = run_plain(&mut peer, None);
        let terminal: Vec<_> = events
            .iter()
            .filter(|event| event.event == kinds::ERROR || event.event == kinds::TURN_COMPLETE)
            .collect();
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].content.as_deref(),
            Some("Internal error: No LLM provider configured")
        );
    }

    #[test]
    fn a_peer_that_exits_mid_turn_reports_its_diagnostic() {
        let (mut exited_peer, _) = peer(vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
        ]);
        let pending = SharedPending::default();
        let events = drive(
            &mut exited_peer,
            None,
            &pending,
            &|| "ERROR boom".to_string(),
            &mut |_| Ok(()),
        );
        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert_eq!(
            events.last().unwrap().content.as_deref(),
            Some("hermes acp exited: ERROR boom")
        );

        let (mut silent, quiet) = peer(vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
        ]);
        let events = run_plain(&mut silent, None);
        assert_eq!(events.last().unwrap().content.as_deref(), Some(EXITED));
        assert_ne!(methods(&quiet), Vec::<String>::new());
    }

    #[test]
    fn a_stop_reason_other_than_end_turn_is_an_error() {
        let (mut peer, _) = peer(vec![
            result(1, capabilities()),
            result(2, json!({ "sessionId": SESSION })),
            result(3, json!({ "stopReason": "refusal" })),
        ]);
        let events = run_plain(&mut peer, None);
        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert_eq!(
            events.last().unwrap().content.as_deref(),
            Some("Hermes refused this turn")
        );
    }
}
