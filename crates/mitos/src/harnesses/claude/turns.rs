use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::controls::{PendingControl, control_response_for, request_from_control};
use super::streams::{process_stream_line, user_message};
use crate::domain::ThreadMode;
use crate::harnesses::{Emitter, Turn};
use crate::ipc::answers::AnswerListener;
use crate::wire::events::{AdapterEvent, kinds};

type SharedPending = Arc<Mutex<HashMap<String, PendingControl>>>;

struct ProcessExit {
    success: bool,
    code: Option<i32>,
}

trait ClaudeProcess {
    type Stdin: Write + Send + 'static;
    type Stdout: std::io::Read + Send + 'static;

    fn take_stdin(&mut self) -> Option<Self::Stdin>;
    fn take_stdout(&mut self) -> Option<Self::Stdout>;
    fn kill(&mut self);
    fn wait(&mut self) -> Result<ProcessExit>;
}

impl ClaudeProcess for std::process::Child {
    type Stdin = ChildStdin;
    type Stdout = std::process::ChildStdout;

    fn take_stdin(&mut self) -> Option<Self::Stdin> {
        self.stdin.take()
    }

    fn take_stdout(&mut self) -> Option<Self::Stdout> {
        self.stdout.take()
    }

    fn kill(&mut self) {
        let _ = self.kill();
    }

    fn wait(&mut self) -> Result<ProcessExit> {
        let status = self.wait()?;
        Ok(ProcessExit {
            success: status.success(),
            code: status.code(),
        })
    }
}

fn permission_mode(mode: ThreadMode) -> &'static str {
    match mode {
        ThreadMode::Plan => "plan",
        ThreadMode::Build => "bypassPermissions",
    }
}

fn arguments(turn: &Turn<'_>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-prompts",
        "none",
        "--permission-mode",
        permission_mode(turn.mode),
    ]
    .map(String::from)
    .into();
    if let Some(session) = turn.session {
        args.extend(["--resume".into(), session.into()]);
    }
    if turn.ephemeral {
        args.push("--no-session-persistence".into());
    }
    args
}

fn send(writer: &mut impl Write, message: &Value) -> Result<()> {
    writeln!(writer, "{message}")?;
    writer.flush()?;
    Ok(())
}

fn send_to_stdin<W: Write>(stdin: &Mutex<Option<W>>, message: &Value) -> Result<()> {
    let mut guard = stdin.lock().expect("claude stdin lock poisoned");
    if let Some(stdin) = guard.as_mut() {
        send(stdin, message)?;
    }
    Ok(())
}

fn answer_handler(
    pending: SharedPending,
    mut send_response: impl FnMut(&Value) -> Result<()> + Send + 'static,
) -> impl FnMut(Value) -> Result<()> + Send + 'static {
    move |answer| {
        if answer.get("action").and_then(Value::as_str) != Some("answer") {
            return Ok(());
        }
        let Some(request_id) = answer.get("request_id").and_then(Value::as_str) else {
            return Ok(());
        };
        let control = pending
            .lock()
            .expect("claude pending lock poisoned")
            .remove(request_id);
        let Some(control) = control else {
            return Ok(());
        };
        let response = answer.get("response").unwrap_or(&Value::Null);
        send_response(&control_response_for(&control, response))
    }
}

pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    run_with_process(spawn_turn(program, turn)?, turn, emitter, answers)
}

fn spawn_turn(program: &OsStr, turn: &Turn<'_>) -> Result<std::process::Child> {
    Command::new(program)
        .args(arguments(turn))
        .current_dir(turn.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("could not start claude")
}

fn run_with_process<P: ClaudeProcess>(
    mut process: P,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let Some(input) = process.take_stdin() else {
        cleanup(&mut process);
        bail!("claude stdin unavailable");
    };
    let Some(stdout) = process.take_stdout() else {
        cleanup(&mut process);
        bail!("claude stdout unavailable");
    };
    let stdin = Arc::new(Mutex::new(Some(input)));
    let pending: SharedPending = Arc::default();

    let outcome = send_to_stdin(&stdin, &user_message(turn.text))
        .context("could not send the message to claude")
        .and_then(|()| {
            let listener = answers
                .map(|path| {
                    AnswerListener::start(
                        path,
                        answer_handler(pending.clone(), {
                            let stdin = stdin.clone();
                            move |message| send_to_stdin(&stdin, message)
                        }),
                    )
                })
                .transpose()?;
            let read = read_events(BufReader::new(stdout), &pending, emitter, || {
                stdin.lock().expect("claude stdin lock poisoned").take();
            });
            drop(listener);
            read
        });
    if outcome.is_err() {
        process.kill();
    }
    stdin.lock().expect("claude stdin lock poisoned").take();
    let status = process.wait()?;
    outcome?;
    if !status.success {
        emitter.emit(exit_error(status.code))?;
    }
    Ok(())
}

fn cleanup(process: &mut impl ClaudeProcess) {
    process.kill();
    let _ = process.wait();
}

fn exit_error(code: Option<i32>) -> AdapterEvent {
    let content = match code {
        Some(code) => format!("claude exited with code {code}"),
        None => "claude was terminated by a signal".into(),
    };
    AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    }
}

fn read_events(
    reader: impl BufRead,
    pending: &SharedPending,
    emitter: &mut Emitter<'_, '_>,
    mut close_stdin: impl FnMut(),
) -> Result<()> {
    for line in reader.lines() {
        let line = line.context("could not read claude output")?;
        let Ok(record) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if let Some((event, control)) = request_from_control(&record) {
            pending
                .lock()
                .expect("claude pending lock poisoned")
                .insert(control.cli_request_id.clone(), control);
            emitter.emit(event)?;
            continue;
        }
        let mut completed = false;
        for event in process_stream_line(&record) {
            completed |= event.event == kinds::TURN_COMPLETE;
            emitter.emit(event)?;
        }
        if completed {
            close_stdin();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Error, ErrorKind};
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[derive(Clone)]
    struct FakeStdin {
        bytes: Arc<Mutex<Vec<u8>>>,
        fails: bool,
    }

    impl Write for FakeStdin {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fails {
                return Err(Error::new(ErrorKind::BrokenPipe, "stdin closed"));
            }
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FakeProcess {
        stdin: Option<FakeStdin>,
        stdout: Option<Cursor<Vec<u8>>>,
        killed: Arc<AtomicBool>,
        exit: ProcessExit,
    }

    impl FakeProcess {
        fn new(output: Option<&str>, stdin: Option<FakeStdin>, exit: ProcessExit) -> Self {
            Self {
                stdin,
                stdout: output.map(|output| Cursor::new(output.as_bytes().to_vec())),
                killed: Arc::new(AtomicBool::new(false)),
                exit,
            }
        }
    }

    impl ClaudeProcess for FakeProcess {
        type Stdin = FakeStdin;
        type Stdout = Cursor<Vec<u8>>;

        fn take_stdin(&mut self) -> Option<Self::Stdin> {
            self.stdin.take()
        }

        fn take_stdout(&mut self) -> Option<Self::Stdout> {
            self.stdout.take()
        }

        fn kill(&mut self) {
            self.killed.store(true, Ordering::Relaxed);
        }

        fn wait(&mut self) -> Result<ProcessExit> {
            Ok(ProcessExit {
                success: self.exit.success,
                code: self.exit.code,
            })
        }
    }

    fn turn() -> Turn<'static> {
        Turn {
            workdir: Path::new("/work"),
            text: "hello",
            session: None,
            mode: ThreadMode::Build,
            ephemeral: false,
            agent: None,
            model: None,
        }
    }

    #[test]
    fn sends_a_stream_json_user_message() {
        use serde_json::json;

        let mut sent = Vec::new();
        send(&mut sent, &user_message("hello")).unwrap();
        let sent: Value = serde_json::from_slice(&sent).unwrap();
        assert_eq!(
            sent,
            json!({ "type": "user", "message": { "role": "user", "content": "hello" } })
        );
    }

    #[test]
    fn parses_stdout_records_tracks_the_session_and_closes_on_completion() {
        let output = concat!(
            "{\"type\":\"system\",\"session_id\":\"s-1\"}\n",
            "not json\n",
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
            "{\"type\":\"result\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}\n",
        );
        let pending = Arc::default();
        let mut seen = Vec::new();
        let (session, completed, outcome) = {
            let mut sink = |event: AdapterEvent| {
                seen.push(event.event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let mut completed = false;
            let outcome = read_events(
                BufReader::new(output.as_bytes()),
                &pending,
                &mut emitter,
                || completed = true,
            );
            (emitter.into_native_session(), completed, outcome)
        };
        outcome.unwrap();
        assert_eq!(
            seen,
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_MESSAGE,
                kinds::USAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(session, Some(serde_json::json!("s-1")));
        assert!(completed);
    }

    #[test]
    fn control_requests_are_emitted_and_known_answers_are_sent() {
        use serde_json::json;

        let pending = Arc::default();
        let mut seen = Vec::new();
        let mut sink = |event: AdapterEvent| {
            seen.push(event.event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);
        read_events(
            BufReader::new(
                br#"{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make clean"}}}"#
                    .as_slice(),
            ),
            &pending,
            &mut emitter,
            || unreachable!("a control request does not complete the turn"),
        )
        .unwrap();
        assert_eq!(seen, [kinds::REQUEST]);

        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut handler = answer_handler(pending.clone(), {
            let sent = sent.clone();
            move |message| send(&mut *sent.lock().unwrap(), message)
        });
        handler(json!({ "action": "started" })).unwrap();
        handler(json!({ "action": "answer", "request_id": "req-404" })).unwrap();
        handler(json!({
            "action": "answer",
            "request_id": "req-1",
            "response": { "allow": true }
        }))
        .unwrap();

        let sent: Value = serde_json::from_slice(&sent.lock().unwrap()).unwrap();
        assert_eq!(
            sent["response"]["response"],
            json!({ "behavior": "allow", "updatedInput": { "command": "make clean" } })
        );
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn formats_process_exit_errors() {
        assert_eq!(
            exit_error(Some(3)).content.as_deref(),
            Some("claude exited with code 3")
        );
        assert_eq!(
            exit_error(None).content.as_deref(),
            Some("claude was terminated by a signal")
        );
    }

    #[test]
    fn arguments_pick_the_permission_mode_and_resume_session() {
        let turn = Turn {
            workdir: Path::new("/w"),
            text: "t",
            session: Some("abc"),
            mode: ThreadMode::Plan,
            ephemeral: false,
            agent: None,
            model: None,
        };
        let args = arguments(&turn);
        let position = args
            .iter()
            .position(|arg| arg == "--permission-mode")
            .unwrap();
        assert_eq!(args[position + 1], "plan");
        assert_eq!(args[args.len() - 2..], ["--resume", "abc"]);
        let build = Turn {
            mode: ThreadMode::Build,
            session: None,
            ..turn
        };
        assert!(arguments(&build).contains(&"bypassPermissions".to_string()));
        assert!(!arguments(&build).contains(&"--no-session-persistence".to_string()));
        let ephemeral = Turn {
            ephemeral: true,
            ..build
        };
        assert!(arguments(&ephemeral).contains(&"--no-session-persistence".to_string()));
    }

    #[test]
    fn in_memory_process_streams_events_and_sends_the_prompt() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let process = FakeProcess::new(
            Some(concat!(
                "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n",
                "{\"type\":\"result\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}\n"
            )),
            Some(FakeStdin {
                bytes: bytes.clone(),
                fails: false,
            }),
            ProcessExit {
                success: true,
                code: Some(0),
            },
        );
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);

        run_with_process(process, &turn(), &mut emitter, None).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&bytes.lock().unwrap()).unwrap(),
            user_message("hello")
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.event.as_str())
                .collect::<Vec<_>>(),
            [kinds::ASSISTANT_MESSAGE, kinds::USAGE, kinds::TURN_COMPLETE]
        );
    }

    #[test]
    fn missing_process_streams_are_cleaned_up() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let process = FakeProcess::new(
            Some(""),
            None,
            ProcessExit {
                success: true,
                code: Some(0),
            },
        );
        let killed = process.killed.clone();
        let mut sink = |_| Ok(());
        let mut emitter = Emitter::new(&mut sink);

        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None)
                .unwrap_err()
                .to_string(),
            "claude stdin unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));

        let process = FakeProcess::new(
            None,
            Some(FakeStdin {
                bytes,
                fails: false,
            }),
            ProcessExit {
                success: true,
                code: Some(0),
            },
        );
        let killed = process.killed.clone();

        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None)
                .unwrap_err()
                .to_string(),
            "claude stdout unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));
    }

    #[test]
    fn failed_input_kills_the_process_and_nonzero_exit_is_an_event() {
        let failed = FakeProcess::new(
            Some(""),
            Some(FakeStdin {
                bytes: Arc::new(Mutex::new(Vec::new())),
                fails: true,
            }),
            ProcessExit {
                success: true,
                code: Some(0),
            },
        );
        let killed = failed.killed.clone();
        let mut sink = |_| Ok(());
        let mut emitter = Emitter::new(&mut sink);

        assert!(run_with_process(failed, &turn(), &mut emitter, None).is_err());
        assert!(killed.load(Ordering::Relaxed));

        let exited = FakeProcess::new(
            Some(""),
            Some(FakeStdin {
                bytes: Arc::new(Mutex::new(Vec::new())),
                fails: false,
            }),
            ProcessExit {
                success: false,
                code: Some(2),
            },
        );
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);

        run_with_process(exited, &turn(), &mut emitter, None).unwrap();

        assert_eq!(
            events[0].content.as_deref(),
            Some("claude exited with code 2")
        );
    }
}
