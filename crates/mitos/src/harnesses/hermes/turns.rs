use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::streams::{
    AcpStream, PendingPermission, REQUEST_PERMISSION, SESSION_UPDATE, outcome_for,
    permission_request,
};
use crate::harnesses::{Emitter, Turn};
use crate::ipc::answers::AnswerListener;
use crate::json::string_value;
use crate::rpc::{Inbound, PeerExited, RpcPeer, RpcWriter};
use crate::wire::events::{AdapterEvent, kinds};

const PROTOCOL_VERSION: u32 = 1;
const METHOD_NOT_FOUND: i64 = -32601;
const EXITED: &str = "hermes acp exited";
const DIAGNOSTIC_WAIT: Duration = Duration::from_millis(200);

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

/// A log line carries a level as a standalone word.
fn is_error_line(line: &str) -> bool {
    ["ERROR", "CRITICAL"].iter().any(|level| {
        line.match_indices(level).any(|(start, _)| {
            let before = line[..start].chars().next_back();
            let after = line[start + level.len()..].chars().next();
            let is_word = |c: char| c.is_alphanumeric() || c == '_';
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
    })
}

fn last_error_line(log: &str) -> String {
    log.lines()
        .rfind(|line| is_error_line(line))
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

/// Runs one turn over `hermes acp`: starts or resumes the session, streams the
/// turn as adapter events, and relays host answers to permission requests.
pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let mut child = Command::new(program)
        .arg("acp")
        .current_dir(turn.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start hermes")?;
    let stdin = child.stdin.take().context("hermes stdin unavailable")?;
    let stdout = child.stdout.take().context("hermes stdout unavailable")?;
    let mut stderr = child.stderr.take().context("hermes stderr unavailable")?;
    let (log_sender, log) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = log_sender.send(text);
    });

    let mut peer = RpcPeer::new(stdin, BufReader::new(stdout), true);
    let writer = peer.writer();
    let pending: SharedPending = Arc::default();
    let outcome = (|| {
        let _listener = answers
            .map(|path| {
                AnswerListener::start(path, answer_handler(writer.clone(), pending.clone()))
            })
            .transpose()?;
        let diagnostic = || {
            log.recv_timeout(DIAGNOSTIC_WAIT)
                .map(|text| last_error_line(&text))
                .unwrap_or_default()
        };
        drive_turn(turn, &mut peer, emitter, &pending, &diagnostic)
    })();
    writer.close();
    let _ = child.kill();
    let _ = child.wait();
    if turn.ephemeral
        && let Some(id) = string_value(emitter.native_session())
    {
        delete_session(program, id);
    }
    outcome
}

/// Hermes has no ephemeral sessions, so an ephemeral turn deletes its session
/// once the ACP process is gone. A failed delete only leaves a stray session.
fn delete_session(program: &OsStr, id: &str) {
    let _ = Command::new(program)
        .args(["sessions", "delete", "--yes", id])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(all(test, unix))]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::domain::ThreadMode;
    use crate::harnesses::fixtures::{FakeProgram, kinds_of};

    const SESSION: &str = "sess-1";

    type Sent = Arc<Mutex<Vec<Value>>>;

    #[derive(Clone)]
    struct Agent {
        sent: Sent,
        out: Arc<Mutex<UnixStream>>,
    }

    impl Agent {
        fn push(&self, message: Value) {
            let mut out = self.out.lock().unwrap();
            let _ = writeln!(out, "{message}");
        }

        fn close(&self) {
            let _ = self.out.lock().unwrap().shutdown(std::net::Shutdown::Both);
        }

        fn methods(&self) -> Vec<String> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .filter_map(|message| message["method"].as_str().map(str::to_string))
                .collect()
        }

        fn find(&self, predicate: impl Fn(&Value) -> bool) -> Option<Value> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .find(|m| predicate(m))
                .cloned()
        }

        /// The agent reads on its own thread, so what the client just wrote may
        /// not be recorded yet.
        fn wait_for(&self, predicate: impl Fn(&Value) -> bool) -> Option<Value> {
            for _ in 0..400 {
                if let Some(found) = self.find(&predicate) {
                    return Some(found);
                }
                thread::sleep(Duration::from_millis(5));
            }
            None
        }
    }

    /// A scripted ACP agent: `respond` sees each client message and returns the
    /// lines the agent sends back.
    fn fake_agent(
        respond: impl Fn(&Value, &Agent) -> Vec<Value> + Send + 'static,
    ) -> (RpcPeer, Agent) {
        let (client, agent_end) = UnixStream::pair().unwrap();
        let peer = RpcPeer::new(client.try_clone().unwrap(), BufReader::new(client), true);
        let agent = Agent {
            sent: Sent::default(),
            out: Arc::new(Mutex::new(agent_end.try_clone().unwrap())),
        };
        let handle = agent.clone();
        thread::spawn(move || {
            for line in BufReader::new(agent_end).lines().map_while(Result::ok) {
                let message: Value = serde_json::from_str(&line).unwrap();
                handle.sent.lock().unwrap().push(message.clone());
                for reply in respond(&message, &handle) {
                    handle.push(reply);
                }
            }
        });
        (peer, agent)
    }

    fn result(message: &Value, value: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": message["id"], "result": value })
    }

    fn notification(update: Value) -> Value {
        json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": SESSION, "update": update } })
    }

    fn capabilities() -> Value {
        json!({ "protocolVersion": 1, "agentCapabilities": { "loadSession": true, "sessionCapabilities": { "resume": {} } } })
    }

    fn standard(message: &Value, agent: &Agent) -> Vec<Value> {
        match message["method"].as_str() {
            Some("initialize") => vec![result(message, capabilities())],
            Some("session/new") => vec![result(message, json!({ "sessionId": SESSION }))],
            Some("session/prompt") => {
                agent.push(notification(json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "Hi there" }
                })));
                vec![result(message, json!({ "stopReason": "end_turn" }))]
            }
            _ => Vec::new(),
        }
    }

    fn drive(
        peer: &mut RpcPeer,
        session: Option<&str>,
        pending: &SharedPending,
        diagnostic: &dyn Fn() -> String,
    ) -> Vec<AdapterEvent> {
        let mut seen = Vec::new();
        {
            let mut sink = |event: AdapterEvent| {
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
            };
            drive_turn(&turn, peer, &mut emitter, pending, diagnostic).unwrap();
        }
        seen
    }

    fn run_plain(peer: &mut RpcPeer, session: Option<&str>) -> Vec<AdapterEvent> {
        drive(peer, session, &SharedPending::default(), &String::new)
    }

    #[test]
    fn a_new_session_streams_a_turn_and_ends_with_one_turn_complete() {
        let (mut peer, agent) = fake_agent(standard);
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
        let sent = agent.sent.lock().unwrap().clone();
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
        let (mut peer, agent) = fake_agent(|message, agent| {
            if message["method"] == "session/resume" {
                vec![result(message, Value::Null)]
            } else {
                standard(message, agent)
            }
        });
        let events = run_plain(&mut peer, Some(SESSION));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let methods = agent.methods();
        assert!(methods.contains(&"session/resume".to_string()));
        assert!(!methods.contains(&"session/new".to_string()));
        assert!(!methods.contains(&"session/load".to_string()));
    }

    #[test]
    fn load_replay_is_not_part_of_the_turn_when_only_session_load_exists() {
        let (mut peer, agent) = fake_agent(|message, agent| match message["method"].as_str() {
            Some("initialize") => vec![result(
                message,
                json!({ "protocolVersion": 1, "agentCapabilities": { "loadSession": true } }),
            )],
            Some("session/load") => {
                agent.push(notification(json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "old history" }
                })));
                vec![result(message, Value::Null)]
            }
            _ => standard(message, agent),
        });
        let events = run_plain(&mut peer, Some(SESSION));
        assert!(agent.methods().contains(&"session/load".to_string()));
        assert!(
            events
                .iter()
                .all(|event| event.content.as_deref() != Some("old history"))
        );
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
    }

    #[test]
    fn an_agent_that_cannot_resume_is_reported_as_an_error() {
        let (mut peer, _) = fake_agent(|message, _| {
            if message["method"] == "initialize" {
                vec![result(
                    message,
                    json!({ "protocolVersion": 1, "agentCapabilities": {} }),
                )]
            } else {
                Vec::new()
            }
        });
        let events = run_plain(&mut peer, Some(SESSION));
        assert_eq!(kinds_of(&events), [kinds::ERROR]);
        assert_eq!(
            events[0].content.as_deref(),
            Some("hermes cannot resume sessions")
        );
    }

    #[test]
    fn a_permission_request_is_answered_with_the_chosen_option() {
        let (mut peer, agent) = fake_agent(|message, agent| {
            if message["method"] != "session/prompt" {
                return standard(message, agent);
            }
            agent.push(json!({
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
            }));
            let waiting = agent.clone();
            let prompt = message.clone();
            thread::spawn(move || {
                while waiting
                    .find(|m| m["id"] == 9 && m.get("result").is_some())
                    .is_none()
                {
                    thread::sleep(Duration::from_millis(5));
                }
                waiting.push(result(&prompt, json!({ "stopReason": "end_turn" })));
            });
            Vec::new()
        });
        let pending = SharedPending::default();
        let answering = {
            let pending = pending.clone();
            let writer = peer.writer();
            thread::spawn(move || {
                while !pending.lock().unwrap().contains_key("9") {
                    thread::sleep(Duration::from_millis(5));
                }
                answer_handler(writer, pending)(json!({
                    "action": "answer",
                    "request_id": "9",
                    "response": { "allow": true }
                }))
                .unwrap();
            })
        };
        let events = drive(&mut peer, None, &pending, &String::new);
        answering.join().unwrap();
        assert!(kinds_of(&events).contains(&kinds::REQUEST));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let reply = agent
            .wait_for(|m| m["id"] == 9 && m.get("result").is_some())
            .unwrap();
        assert_eq!(
            reply["result"],
            json!({ "outcome": { "outcome": "selected", "optionId": "ao" } })
        );
    }

    #[test]
    fn an_unknown_agent_request_is_rejected() {
        let (mut peer, agent) = fake_agent(|message, agent| {
            if message["method"] == "session/prompt" {
                agent.push(json!({
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "fs/read_text_file",
                    "params": { "sessionId": SESSION, "path": "/etc/passwd" }
                }));
            }
            standard(message, agent)
        });
        run_plain(&mut peer, None);
        let rejection = agent
            .wait_for(|m| m["id"] == 5 && m.get("error").is_some())
            .unwrap();
        assert_eq!(rejection["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn an_rpc_error_is_the_single_terminal_error_with_the_servers_details() {
        let (mut peer, _) = fake_agent(|message, _| {
            if message["method"] == "initialize" {
                return vec![result(message, capabilities())];
            }
            vec![json!({
                "jsonrpc": "2.0",
                "id": message["id"],
                "error": {
                    "code": -32603,
                    "message": "Internal error",
                    "data": { "details": "No LLM provider configured" }
                }
            })]
        });
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
    fn a_process_that_exits_mid_turn_reports_why_from_its_diagnostic() {
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let (mut peer, _) = fake_agent(move |message, agent| {
            if message["method"] == "session/prompt" {
                flag.store(true, Ordering::SeqCst);
                agent.close();
                return Vec::new();
            }
            standard(message, agent)
        });
        let events = drive(&mut peer, None, &SharedPending::default(), &|| {
            "ERROR boom".to_string()
        });
        assert!(closed.load(Ordering::SeqCst));
        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert_eq!(
            events.last().unwrap().content.as_deref(),
            Some("hermes acp exited: ERROR boom")
        );

        let (mut silent, quiet) = fake_agent(|message, agent| {
            if message["method"] == "session/prompt" {
                agent.close();
                return Vec::new();
            }
            standard(message, agent)
        });
        let events = run_plain(&mut silent, None);
        assert_eq!(events.last().unwrap().content.as_deref(), Some(EXITED));
        assert_ne!(quiet.methods(), Vec::<String>::new());
    }

    #[test]
    fn a_stop_reason_other_than_end_turn_is_an_error() {
        let (mut peer, _) = fake_agent(|message, agent| {
            if message["method"] == "session/prompt" {
                vec![result(message, json!({ "stopReason": "refusal" }))]
            } else {
                standard(message, agent)
            }
        });
        let events = run_plain(&mut peer, None);
        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert_eq!(
            events.last().unwrap().content.as_deref(),
            Some("Hermes refused this turn")
        );
    }

    #[test]
    fn error_lines_match_levels_as_whole_words() {
        assert!(is_error_line("2026 ERROR boom"));
        assert!(is_error_line("[CRITICAL] down"));
        assert!(!is_error_line("NOERROR here"));
        assert!(!is_error_line("ERRORS happened"));
        assert_eq!(
            last_error_line("a\nERROR first\nok\nCRITICAL last \n"),
            "CRITICAL last"
        );
        assert_eq!(last_error_line("nothing"), "");
    }

    #[test]
    fn the_real_process_path_reports_stderr_when_the_program_dies() {
        let fake = FakeProgram::new("read line\necho 'ERROR provider missing' >&2\nexit 1");
        let mut seen = Vec::new();
        {
            let mut sink = |event: AdapterEvent| {
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let turn = Turn {
                workdir: &fake.dir,
                text: "hello",
                session: None,
                mode: ThreadMode::Build,
                ephemeral: false,
            };
            run_turn(fake.path.as_os_str(), &turn, &mut emitter, None).unwrap();
        }
        assert_eq!(kinds_of(&seen), [kinds::ERROR]);
        assert_eq!(
            seen[0].content.as_deref(),
            Some("hermes acp exited: ERROR provider missing")
        );
    }

    const ACP_SCRIPT: &str = r#"dir="$(dirname "$0")"
if [ "$1" = sessions ]; then
  printf '%s' "$*" > "$dir/deleted"
  exit 0
fi
read line
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}'
read line
echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s-e"}}'
read line
echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
cat >/dev/null"#;

    fn run_scripted(fake: &FakeProgram, ephemeral: bool) {
        let mut sink = |_: AdapterEvent| Ok(());
        let mut emitter = Emitter::new(&mut sink);
        let turn = Turn {
            workdir: &fake.dir,
            text: "hello",
            session: None,
            mode: ThreadMode::Build,
            ephemeral,
        };
        run_turn(fake.path.as_os_str(), &turn, &mut emitter, None).unwrap();
    }

    #[test]
    fn an_ephemeral_turn_deletes_its_session_through_the_cli() {
        let fake = FakeProgram::new(ACP_SCRIPT);
        run_scripted(&fake, true);
        assert_eq!(
            std::fs::read_to_string(fake.dir.join("deleted")).unwrap(),
            "sessions delete --yes s-e"
        );
    }

    #[test]
    fn a_persistent_turn_keeps_its_session() {
        let fake = FakeProgram::new(ACP_SCRIPT);
        run_scripted(&fake, false);
        assert!(!fake.dir.join("deleted").exists());
    }
}
