use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

use super::clients::{SERVER_USER, ServerAccess, ServerClient};
use super::streams::{PendingAsk, SessionStream, agent_for, pending_for, reply_for};
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::ipc::answers::AnswerListener;
use crate::wire::events::{AdapterEvent, kinds};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const LISTENING: &str = "listening on ";
const STREAM_ENDED: &str = "opencode event stream ended before the turn completed";

pub struct Turn<'a> {
    pub workdir: &'a Path,
    pub text: &'a str,
    pub session: Option<&'a str>,
    pub mode: ThreadMode,
    pub ephemeral: bool,
}

pub type SharedPending = Arc<Mutex<HashMap<String, PendingAsk>>>;

pub fn answer_handler(
    client: ServerClient,
    pending: SharedPending,
) -> impl FnMut(Value) -> Result<()> + Send + 'static {
    move |answer| {
        if answer.get("action").and_then(Value::as_str) != Some("answer") {
            return Ok(());
        }
        let Some(request_id) = answer.get("request_id").and_then(Value::as_str) else {
            return Ok(());
        };
        let ask = pending
            .lock()
            .expect("opencode pending lock poisoned")
            .remove(request_id);
        let Some(ask) = ask else {
            return Ok(());
        };
        let response = answer.get("response").unwrap_or(&Value::Null);
        let reply = reply_for(&ask, response);
        let _ = client.post(&reply.path, &reply.body);
        Ok(())
    }
}

fn error_event(content: String) -> AdapterEvent {
    AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    }
}

fn run(
    client: &ServerClient,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    let session_id = match turn.session {
        Some(existing) => existing.to_string(),
        None => client.create_session()?,
    };
    let outcome = stream_turn(client, turn, &session_id, emitter, pending);
    if turn.ephemeral {
        let _ = client.delete_session(&session_id);
    }
    outcome
}

fn stream_turn(
    client: &ServerClient,
    turn: &Turn<'_>,
    session_id: &str,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    emitter.emit(AdapterEvent {
        native_session: Some(json!(session_id)),
        ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
    })?;

    // Subscribe before prompting so no early event is missed.
    let events = client.subscribe()?;
    client.post(
        &format!("/session/{session_id}/prompt_async"),
        &json!({
            "agent": agent_for(turn.mode),
            "parts": [{ "type": "text", "text": turn.text }],
        }),
    )?;

    let mut stream = SessionStream::new(session_id);
    for raw in events {
        let step = stream.process(&raw);
        for event in step.events {
            if event.event == kinds::REQUEST
                && let Some(ask) = pending_for(&event)
            {
                pending
                    .lock()
                    .expect("opencode pending lock poisoned")
                    .insert(ask.id.clone(), ask);
            }
            emitter.emit(event)?;
        }
        if step.done {
            return Ok(());
        }
    }
    emitter.emit(error_event(STREAM_ENDED.into()))
}

/// Drives one turn against a running server: the caller owns the server
/// process. Emits exactly one terminal event.
pub fn drive_turn(
    client: &ServerClient,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    match run(client, turn, emitter, pending) {
        Ok(()) => Ok(()),
        Err(cause) => emitter.emit(error_event(cause.to_string())),
    }
}

fn listening_url(line: &str) -> Option<String> {
    let rest = &line[line.find(LISTENING)? + LISTENING.len()..];
    let url = rest.split_whitespace().next()?;
    (url.starts_with("http://") || url.starts_with("https://")).then(|| url.to_string())
}

/// Forwards the server's URL once it logs where it is listening. Stdout keeps
/// draining afterwards so the child never blocks on a full pipe.
fn forward_listening_url(stdout: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut announced = false;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if !announced && let Some(url) = listening_url(&line) {
                announced = true;
                let _ = sender.send(url);
            }
        }
    });
    receiver
}

fn read_all(mut source: impl Read + Send + 'static) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        let _ = source.read_to_string(&mut text);
        text
    })
}

/// Runs one turn over a private `opencode serve`: creates or resumes the
/// session, streams the turn as adapter events, and relays host answers to
/// permission and question asks.
pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let password = uuid::Uuid::new_v4().to_string();
    let mut child = Command::new(program)
        .args(["serve", "--port", "0", "--hostname", "127.0.0.1"])
        .current_dir(turn.workdir)
        .env("OPENCODE_SERVER_USERNAME", SERVER_USER)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start opencode")?;
    let stdout = child.stdout.take().context("opencode stdout unavailable")?;
    let stderr = child.stderr.take().context("opencode stderr unavailable")?;
    let diagnostics = read_all(stderr);
    let urls = forward_listening_url(stdout);

    let started = match urls.recv_timeout(STARTUP_TIMEOUT) {
        Ok(base) => Ok(ServerAccess { base, password }),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(anyhow!("opencode serve did not start in time"))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(match child.wait()?.code() {
            Some(code) => anyhow!("opencode serve exited with code {code}"),
            None => anyhow!("opencode serve exited before listening"),
        }),
    };

    let outcome = match started {
        Ok(access) => {
            let client = ServerClient::new(&access, &turn.workdir.to_string_lossy());
            let pending: SharedPending = Arc::default();
            let listener = answers
                .map(|path| {
                    AnswerListener::start(path, answer_handler(client.clone(), pending.clone()))
                })
                .transpose();
            match listener {
                Ok(_listener) => drive_turn(&client, turn, emitter, &pending),
                Err(cause) => emitter.emit(error_event(cause.to_string())),
            }
        }
        Err(cause) => {
            let already_exited = child.try_wait().ok().flatten().is_some();
            let diagnostic = if already_exited {
                diagnostics.join().unwrap_or_default().trim().to_string()
            } else {
                String::new()
            };
            let content = if diagnostic.is_empty() {
                cause.to_string()
            } else {
                diagnostic
            };
            emitter.emit(error_event(content))
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    outcome
}

#[cfg(all(test, unix))]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use std::io::Write;
    use std::net::{Shutdown, TcpListener, TcpStream};

    use super::*;
    use crate::harnesses::fixtures::{FakeProgram, kinds_of};
    use crate::harnesses::opencode::clients::base64;

    const SESSION: &str = "ses_fake";

    #[derive(Clone)]
    struct Recorded {
        path: String,
        body: Value,
        authorization: Option<String>,
    }

    type OnPrompt = Arc<dyn Fn(&Fake) + Send + Sync>;

    /// A fake `opencode serve`: `on_prompt` runs once each prompt is accepted.
    #[derive(Clone)]
    struct Fake {
        base: String,
        requests: Arc<Mutex<Vec<Recorded>>>,
        sse: Arc<Mutex<Option<TcpStream>>>,
        on_prompt: OnPrompt,
    }

    impl Fake {
        fn start(on_prompt: impl Fn(&Fake) + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let fake = Self {
                base: format!("http://{}", listener.local_addr().unwrap()),
                requests: Arc::default(),
                sse: Arc::default(),
                on_prompt: Arc::new(on_prompt),
            };
            let server = fake.clone();
            thread::spawn(move || {
                for stream in listener.incoming().map_while(Result::ok) {
                    let server = server.clone();
                    thread::spawn(move || server.serve(stream));
                }
            });
            fake
        }

        fn serve(&self, mut stream: TcpStream) {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let target = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();
            let (mut length, mut authorization) = (0usize, None);
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim();
                if header.is_empty() {
                    break;
                }
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                } else if lower.starts_with("authorization:") {
                    authorization = Some(header["authorization:".len()..].trim().to_string());
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let path = target.split('?').next().unwrap_or("").to_string();
            self.requests.lock().unwrap().push(Recorded {
                path: path.clone(),
                body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                authorization: authorization.clone(),
            });
            let respond = |stream: &mut TcpStream, status: &str, body: &str| {
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            };
            if authorization.is_none() {
                return respond(&mut stream, "401 Unauthorized", "unauthorized");
            }
            if path == "/event" {
                *self.sse.lock().unwrap() = Some(stream.try_clone().unwrap());
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n\
                     data: {{\"type\":\"server.connected\",\"properties\":{{}}}}\n\n"
                );
            } else if path == "/session" {
                respond(&mut stream, "200 OK", &format!("{{\"id\":\"{SESSION}\"}}"));
            } else if path.ends_with("/prompt_async") {
                respond(&mut stream, "204 No Content", "");
                (self.on_prompt)(self);
            } else {
                respond(&mut stream, "200 OK", "true");
            }
        }

        fn push(&self, kind: &str, properties: Value) {
            let mut properties = properties.as_object().unwrap().clone();
            properties.insert("sessionID".into(), json!(SESSION));
            let frame = json!({ "type": kind, "properties": properties });
            if let Some(stream) = self.sse.lock().unwrap().as_mut() {
                let _ = write!(stream, "data: {frame}\n\n");
                let _ = stream.flush();
            }
        }

        fn close(&self) {
            if let Some(stream) = self.sse.lock().unwrap().take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }

        /// The server handles each connection on its own thread, so a request
        /// the client just made may not be recorded yet.
        fn request(&self, matches: impl Fn(&Recorded) -> bool) -> Option<Recorded> {
            for _ in 0..400 {
                if let Some(found) = self.requests.lock().unwrap().iter().find(|r| matches(r)) {
                    return Some(found.clone());
                }
                thread::sleep(Duration::from_millis(5));
            }
            None
        }

        fn client(&self) -> ServerClient {
            ServerClient::new(
                &ServerAccess {
                    base: self.base.clone(),
                    password: "pw".into(),
                },
                "/work",
            )
        }
    }

    fn finish_with_a_reply(fake: &Fake) {
        fake.push(
            "message.updated",
            json!({ "info": { "id": "msg_a", "role": "assistant", "modelID": "m" } }),
        );
        fake.push(
            "message.part.updated",
            json!({ "part": { "id": "prt_1", "type": "text", "messageID": "msg_a", "text": "done", "time": { "start": 1, "end": 2 } } }),
        );
        fake.push("session.idle", json!({}));
        fake.push("session.idle", json!({}));
    }

    fn drive(
        client: &ServerClient,
        session: Option<&str>,
        mode: ThreadMode,
        ephemeral: bool,
        pending: &SharedPending,
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
                mode,
                ephemeral,
            };
            drive_turn(client, &turn, &mut emitter, pending).unwrap();
        }
        seen
    }

    fn drive_plain(fake: &Fake, session: Option<&str>, mode: ThreadMode) -> Vec<AdapterEvent> {
        drive(
            &fake.client(),
            session,
            mode,
            false,
            &SharedPending::default(),
        )
    }

    #[test]
    fn an_ephemeral_turn_deletes_its_session_afterwards() {
        let fake = Fake::start(finish_with_a_reply);
        drive(
            &fake.client(),
            None,
            ThreadMode::Plan,
            true,
            &SharedPending::default(),
        );
        assert!(
            fake.request(|r| r.path == format!("/session/{SESSION}"))
                .is_some()
        );
    }

    #[test]
    fn a_persistent_turn_keeps_its_session() {
        let fake = Fake::start(finish_with_a_reply);
        drive_plain(&fake, None, ThreadMode::Build);
        assert!(
            fake.requests
                .lock()
                .unwrap()
                .iter()
                .all(|r| r.path != format!("/session/{SESSION}"))
        );
    }

    #[test]
    fn a_new_session_streams_a_turn_and_ends_with_one_turn_complete() {
        let fake = Fake::start(finish_with_a_reply);
        let events = drive_plain(&fake, None, ThreadMode::Build);
        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_MESSAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(events[0].native_session, Some(json!(SESSION)));
        let prompt = fake.request(|r| r.path.ends_with("/prompt_async")).unwrap();
        assert_eq!(
            prompt.body,
            json!({ "agent": "build", "parts": [{ "type": "text", "text": "hello" }] })
        );
    }

    #[test]
    fn an_existing_session_is_resumed_without_creating_one() {
        let fake = Fake::start(|fake| {
            fake.push(
                "message.updated",
                json!({ "info": { "id": "msg_a", "role": "assistant" } }),
            );
            fake.push("session.idle", json!({}));
        });
        let events = drive_plain(&fake, Some(SESSION), ThreadMode::Plan);
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let prompt = fake.request(|r| r.path.ends_with("/prompt_async")).unwrap();
        assert_eq!(prompt.body["agent"], "plan");
        assert!(
            fake.requests
                .lock()
                .unwrap()
                .iter()
                .all(|r| r.path != "/session")
        );
    }

    #[test]
    fn a_permission_ask_is_answered_over_the_permission_endpoint() {
        let fake = Fake::start(|fake| {
            fake.push(
                "permission.asked",
                json!({ "id": "per_1", "permission": "bash", "patterns": ["ls"] }),
            );
            let waiting = fake.clone();
            thread::spawn(move || {
                waiting.request(|r| r.path == "/permission/per_1/reply");
                waiting.push("session.idle", json!({}));
            });
        });
        let client = fake.client();
        let pending = SharedPending::default();
        let answering = {
            let (client, pending) = (client.clone(), pending.clone());
            thread::spawn(move || {
                while !pending.lock().unwrap().contains_key("per_1") {
                    thread::sleep(Duration::from_millis(5));
                }
                answer_handler(client, pending)(json!({
                    "action": "answer",
                    "request_id": "per_1",
                    "response": { "allow": true }
                }))
                .unwrap();
            })
        };
        let events = drive(&client, None, ThreadMode::Build, false, &pending);
        answering.join().unwrap();
        assert!(kinds_of(&events).contains(&kinds::REQUEST));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let reply = fake
            .request(|r| r.path == "/permission/per_1/reply")
            .unwrap();
        assert_eq!(reply.body, json!({ "reply": "once" }));
    }

    #[test]
    fn a_session_error_ends_the_turn_with_exactly_one_error() {
        let fake = Fake::start(|fake| {
            fake.push(
                "session.error",
                json!({ "error": { "name": "UnknownError", "data": { "message": "boom" } } }),
            );
            fake.push("session.idle", json!({}));
        });
        let events = drive_plain(&fake, None, ThreadMode::Build);
        let terminal: Vec<_> = events
            .iter()
            .filter(|event| event.event == kinds::ERROR || event.event == kinds::TURN_COMPLETE)
            .collect();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].content.as_deref(), Some("boom"));
    }

    #[test]
    fn a_stream_that_closes_early_is_reported_as_an_error() {
        let fake = Fake::start(Fake::close);
        let events = drive_plain(&fake, None, ThreadMode::Build);
        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert!(
            events
                .last()
                .unwrap()
                .content
                .as_deref()
                .unwrap()
                .contains("event stream ended")
        );
    }

    #[test]
    fn an_unreachable_server_is_reported_as_an_error() {
        let mut fake = Fake::start(|_| {});
        fake.base = "http://127.0.0.1:1".into();
        let events = drive_plain(&fake, None, ThreadMode::Build);
        assert_eq!(kinds_of(&events), [kinds::ERROR]);
    }

    #[test]
    fn the_server_is_started_with_a_private_password_and_found_by_its_log_line() {
        let fake = Fake::start(finish_with_a_reply);
        let program = FakeProgram::new(&format!(
            r#"dir="$(dirname "$0")"
printf '%s:%s' "$OPENCODE_SERVER_USERNAME" "$OPENCODE_SERVER_PASSWORD" > "$dir/creds"
printf '%s ' "$@" > "$dir/args"
echo "opencode server listening on {}"
"#,
            fake.base
        ));
        let mut seen = Vec::new();
        {
            let mut sink = |event: AdapterEvent| {
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let turn = Turn {
                workdir: &program.dir,
                text: "hello",
                session: None,
                mode: ThreadMode::Build,
                ephemeral: false,
            };
            run_turn(program.path.as_os_str(), &turn, &mut emitter, None).unwrap();
        }
        assert_eq!(seen.last().unwrap().event, kinds::TURN_COMPLETE);
        let creds = std::fs::read_to_string(program.dir.join("creds")).unwrap();
        assert!(creds.starts_with("mitos:") && creds.len() > "mitos:".len());
        let args = std::fs::read_to_string(program.dir.join("args")).unwrap();
        assert_eq!(args, "serve --port 0 --hostname 127.0.0.1 ");
        let session = fake.request(|r| r.path == "/session").unwrap();
        assert_eq!(
            session.authorization,
            Some(format!("Basic {}", base64(creds.as_bytes())))
        );
    }

    #[test]
    fn a_server_that_exits_before_listening_reports_stderr_or_its_exit_code() {
        fn failure(program: &FakeProgram) -> String {
            let mut seen = Vec::new();
            {
                let mut sink = |event: AdapterEvent| {
                    seen.push(event);
                    Ok(())
                };
                let mut emitter = Emitter::new(&mut sink);
                let turn = Turn {
                    workdir: &program.dir,
                    text: "hello",
                    session: None,
                    mode: ThreadMode::Build,
                    ephemeral: false,
                };
                run_turn(program.path.as_os_str(), &turn, &mut emitter, None).unwrap();
            }
            assert_eq!(kinds_of(&seen), [kinds::ERROR]);
            seen[0].content.clone().unwrap()
        }

        assert_eq!(
            failure(&FakeProgram::new("exit 3")),
            "opencode serve exited with code 3"
        );
        assert_eq!(
            failure(&FakeProgram::new("echo 'provider missing' >&2\nexit 3")),
            "provider missing"
        );
    }

    #[test]
    fn the_listening_url_is_the_first_http_url_after_the_marker() {
        assert_eq!(
            listening_url("opencode server listening on http://127.0.0.1:4096 ok").as_deref(),
            Some("http://127.0.0.1:4096")
        );
        assert_eq!(listening_url("listening on ftp://x"), None);
        assert_eq!(listening_url("starting up"), None);
        assert_eq!(listening_url("listening on "), None);
    }
}
