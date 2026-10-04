use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use super::requests::{PendingServerRequest, request_from_server, result_for};
use super::streams::{process_notification, process_turn_completed};
use super::tokens::TurnTokens;
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::ipc::answers::AnswerListener;
use crate::rpc::{Inbound, RpcPeer, RpcWriter};
use crate::wire::events::{AdapterEvent, kinds};

const METHOD_NOT_FOUND: i64 = -32601;
const NO_THREAD_ID: &str = "codex did not return a thread id";
const NOT_COMPLETED: &str = "codex ended without completing the turn";

pub struct Turn<'a> {
    pub workdir: &'a Path,
    pub text: &'a str,
    pub session: Option<&'a str>,
    pub mode: ThreadMode,
    pub ephemeral: bool,
}

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

fn read_stderr(handle: JoinHandle<String>) -> String {
    handle.join().unwrap_or_default().trim().to_string()
}

/// Runs one turn over `codex app-server`: starts or resumes the thread,
/// streams the turn as adapter events, and relays host answers back to
/// server-initiated requests. Emits exactly one terminal event.
pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let mut child = Command::new(program)
        .arg("app-server")
        .current_dir(turn.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start codex")?;
    let stdin = child.stdin.take().context("codex stdin unavailable")?;
    let stdout = child.stdout.take().context("codex stdout unavailable")?;
    let mut stderr = child.stderr.take().context("codex stderr unavailable")?;
    let diagnostics = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let mut peer = RpcPeer::new(stdin, BufReader::new(stdout), false);
    let writer = peer.writer();
    let pending: SharedPending = Arc::default();
    let finished = Cell::new(false);
    let mut session = Session {
        emitter,
        writer: writer.clone(),
        pending: pending.clone(),
        tokens: TurnTokens::default(),
        finished: &finished,
    };

    let outcome = (|| {
        let _listener = answers
            .map(|path| {
                AnswerListener::start(path, answer_handler(writer.clone(), pending.clone()))
            })
            .transpose()?;
        drive(turn, &mut peer, &mut session)
    })();

    let already_exited = child.try_wait().ok().flatten().is_some();
    let reported = match outcome {
        Ok(()) if finished.get() => Ok(()),
        Ok(()) => {
            let diagnostic = read_stderr(diagnostics);
            let content = if diagnostic.is_empty() {
                NOT_COMPLETED.into()
            } else {
                diagnostic
            };
            session.emitter.emit(error_event(content))
        }
        Err(cause) => {
            let diagnostic = if already_exited {
                read_stderr(diagnostics)
            } else {
                String::new()
            };
            let content = if diagnostic.is_empty() {
                cause.to_string()
            } else {
                diagnostic
            };
            session.emitter.emit(error_event(content))
        }
    };
    writer.close();
    let _ = child.kill();
    let _ = child.wait();
    reported
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::harnesses::fixtures::{FakeProgram, kinds_of};

    const HANDSHAKE: &str = r#"read line
echo '{"id":1,"result":{}}'
read line
read line
echo '{"id":2,"result":{"thread":{"id":"th-1"}}}'
read line
echo '{"id":3,"result":{}}'"#;

    fn run(
        fake: &FakeProgram,
        session: Option<&str>,
    ) -> (Vec<AdapterEvent>, Option<Value>, Result<()>) {
        let mut seen = Vec::new();
        let (native, outcome) = {
            let mut sink = |event: AdapterEvent| {
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let turn = Turn {
                workdir: &fake.dir,
                text: "hello",
                session,
                mode: ThreadMode::Build,
                ephemeral: false,
            };
            let outcome = run_turn(fake.path.as_os_str(), &turn, &mut emitter, None);
            (emitter.into_native_session(), outcome)
        };
        (seen, native, outcome)
    }

    #[test]
    fn a_turn_streams_notifications_and_ends_at_turn_completed() {
        let fake = FakeProgram::new(&format!(
            r#"{HANDSHAKE}
echo '{{"method":"item/agentMessage/delta","params":{{"delta":"Hi"}}}}'
echo '{{"method":"item/completed","params":{{"item":{{"type":"agentMessage","text":"Done."}}}}}}'
echo '{{"method":"turn/completed","params":{{"turn":{{"status":"completed"}}}}}}'
cat >/dev/null"#
        ));
        let (seen, native, outcome) = run(&fake, None);
        outcome.unwrap();
        assert_eq!(
            kinds_of(&seen),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_DELTA,
                kinds::ASSISTANT_MESSAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(native, Some(json!("th-1")));
    }

    #[test]
    fn the_requests_carry_the_session_mode_and_prompt() {
        let fake = FakeProgram::new(
            r#"dir="$(dirname "$0")"
read line
echo '{"id":1,"result":{}}'
read line
read line
printf '%s' "$line" > "$dir/thread"
echo '{"id":2,"result":{"thread":{"id":"th-9"}}}'
read line
printf '%s' "$line" > "$dir/turn"
echo '{"id":3,"result":{}}'
echo '{"method":"turn/completed","params":{"turn":{"status":"completed"}}}'
cat >/dev/null"#,
        );
        let (_, native, outcome) = run(&fake, Some("th-9"));
        outcome.unwrap();
        assert_eq!(native, Some(json!("th-9")));
        let thread: Value =
            serde_json::from_str(&std::fs::read_to_string(fake.dir.join("thread")).unwrap())
                .unwrap();
        assert_eq!(thread["method"], "thread/resume");
        assert_eq!(thread["params"]["threadId"], "th-9");
        assert_eq!(thread["params"]["excludeTurns"], true);
        assert_eq!(thread["params"]["sandbox"], "danger-full-access");
        assert_eq!(thread["params"]["approvalPolicy"], "never");
        let turn: Value =
            serde_json::from_str(&std::fs::read_to_string(fake.dir.join("turn")).unwrap()).unwrap();
        assert_eq!(turn["method"], "turn/start");
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
    fn an_ephemeral_turn_starts_an_ephemeral_thread() {
        let fake = FakeProgram::new(
            r#"dir="$(dirname "$0")"
read line
echo '{"id":1,"result":{}}'
read line
read line
printf '%s' "$line" > "$dir/thread"
echo '{"id":2,"result":{"thread":{"id":"th-e"}}}'
read line
echo '{"id":3,"result":{}}'
echo '{"method":"turn/completed","params":{"turn":{"status":"completed"}}}'
cat >/dev/null"#,
        );
        let mut sink = |_: AdapterEvent| Ok(());
        let mut emitter = Emitter::new(&mut sink);
        let turn = Turn {
            workdir: &fake.dir,
            text: "hello",
            session: None,
            mode: ThreadMode::Plan,
            ephemeral: true,
        };
        run_turn(fake.path.as_os_str(), &turn, &mut emitter, None).unwrap();
        let thread: Value =
            serde_json::from_str(&std::fs::read_to_string(fake.dir.join("thread")).unwrap())
                .unwrap();
        assert_eq!(thread["method"], "thread/start");
        assert_eq!(thread["params"]["ephemeral"], true);
        assert_eq!(thread["params"]["sandbox"], "read-only");
    }

    #[test]
    fn unanswerable_server_requests_are_rejected() {
        let fake = FakeProgram::new(&format!(
            r#"{HANDSHAKE}
echo '{{"id":"s1","method":"attestation/generate","params":{{}}}}'
read line
printf '%s' "$line" > "$(dirname "$0")/rejection"
echo '{{"method":"turn/completed","params":{{"turn":{{"status":"completed"}}}}}}'
cat >/dev/null"#
        ));
        let (seen, _, outcome) = run(&fake, None);
        outcome.unwrap();
        assert_eq!(
            kinds_of(&seen),
            [kinds::NATIVE_SESSION_UPDATE, kinds::TURN_COMPLETE]
        );
        let rejection: Value =
            serde_json::from_str(&std::fs::read_to_string(fake.dir.join("rejection")).unwrap())
                .unwrap();
        assert_eq!(rejection["id"], "s1");
        assert_eq!(rejection["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn a_server_request_becomes_a_request_event() {
        let fake = FakeProgram::new(&format!(
            r#"{HANDSHAKE}
echo '{{"id":5,"method":"item/commandExecution/requestApproval","params":{{"command":"ls"}}}}'
echo '{{"method":"turn/completed","params":{{"turn":{{"status":"completed"}}}}}}'
cat >/dev/null"#
        ));
        let (seen, _, outcome) = run(&fake, None);
        outcome.unwrap();
        assert_eq!(
            kinds_of(&seen),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::REQUEST,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(seen[1].payload.as_ref().unwrap()["id"], "5");
    }

    #[test]
    fn ending_without_turn_completed_reports_stderr_or_a_default() {
        {
            let with_stderr = FakeProgram::new(&format!("{HANDSHAKE}\necho boom >&2"));
            let (seen, _, outcome) = run(&with_stderr, None);
            outcome.unwrap();
            assert_eq!(
                kinds_of(&seen),
                [kinds::NATIVE_SESSION_UPDATE, kinds::ERROR]
            );
            assert_eq!(seen[1].content.as_deref(), Some("boom"));
        }

        let silent = FakeProgram::new(HANDSHAKE);
        let (seen, _, _) = run(&silent, None);
        assert_eq!(seen.last().unwrap().content.as_deref(), Some(NOT_COMPLETED));
    }

    #[test]
    fn a_missing_thread_id_is_an_error_event() {
        let fake = FakeProgram::new(
            r#"read line
echo '{"id":1,"result":{}}'
read line
read line
echo '{"id":2,"result":{}}'
cat >/dev/null"#,
        );
        let (seen, _, outcome) = run(&fake, None);
        outcome.unwrap();
        assert_eq!(kinds_of(&seen), [kinds::ERROR]);
        assert_eq!(seen[0].content.as_deref(), Some(NO_THREAD_ID));
    }

    #[test]
    fn a_failed_turn_surfaces_the_servers_error() {
        let fake = FakeProgram::new(&format!(
            r#"{HANDSHAKE}
echo '{{"method":"turn/completed","params":{{"turn":{{"status":"failed","error":{{"message":"nope"}}}}}}}}'
cat >/dev/null"#
        ));
        let (seen, _, outcome) = run(&fake, None);
        outcome.unwrap();
        assert_eq!(
            kinds_of(&seen),
            [kinds::NATIVE_SESSION_UPDATE, kinds::ERROR]
        );
        assert_eq!(seen[1].content.as_deref(), Some("nope"));
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
}
