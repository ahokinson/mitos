use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde_json::Value;

use super::controls::{PendingControl, control_response_for, request_from_control};
use super::streams::{process_stream_line, user_message};
use crate::adapters::Emitter;
use crate::domain::ThreadMode;
use crate::ipc::answers::AnswerListener;
use crate::wire::events::{AdapterEvent, kinds};

pub struct Turn<'a> {
    pub workdir: &'a Path,
    pub text: &'a str,
    pub resume: Option<&'a str>,
    pub mode: ThreadMode,
}

type SharedStdin = Arc<Mutex<Option<ChildStdin>>>;
type SharedPending = Arc<Mutex<HashMap<String, PendingControl>>>;

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
    if let Some(session) = turn.resume {
        args.extend(["--resume".into(), session.into()]);
    }
    args
}

fn send(stdin: &Mutex<Option<ChildStdin>>, message: &Value) -> Result<()> {
    let mut guard = stdin.lock().expect("claude stdin lock poisoned");
    if let Some(stdin) = guard.as_mut() {
        writeln!(stdin, "{message}")?;
        stdin.flush()?;
    }
    Ok(())
}

fn answer_handler(
    stdin: SharedStdin,
    pending: SharedPending,
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
        send(&stdin, &control_response_for(&control, response))
    }
}

pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let mut child = Command::new(program)
        .args(arguments(turn))
        .current_dir(turn.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("could not start claude")?;
    let stdin: SharedStdin = Arc::new(Mutex::new(child.stdin.take()));
    let stdout = child.stdout.take().context("claude stdout unavailable")?;
    let pending: SharedPending = Arc::default();

    let outcome = send(&stdin, &user_message(turn.text))
        .context("could not send the message to claude")
        .and_then(|()| {
            let listener = answers
                .map(|path| {
                    AnswerListener::start(path, answer_handler(stdin.clone(), pending.clone()))
                })
                .transpose()?;
            let read = read_events(BufReader::new(stdout), &stdin, &pending, emitter);
            drop(listener);
            read
        });
    if outcome.is_err() {
        let _ = child.kill();
    }
    stdin.lock().expect("claude stdin lock poisoned").take();
    let status = child.wait()?;
    outcome?;
    if !status.success() {
        let content = match status.code() {
            Some(code) => format!("claude exited with code {code}"),
            None => "claude was terminated by a signal".into(),
        };
        emitter.emit(AdapterEvent {
            content: Some(content),
            ..AdapterEvent::new(kinds::ERROR)
        })?;
    }
    Ok(())
}

fn read_events(
    reader: impl BufRead,
    stdin: &SharedStdin,
    pending: &SharedPending,
    emitter: &mut Emitter<'_, '_>,
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
            stdin.lock().expect("claude stdin lock poisoned").take();
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::harnesses::fixtures::FakeProgram;

    fn run(path: &Path, dir: &Path) -> (Vec<String>, Option<Value>, Result<()>) {
        let mut seen = Vec::new();
        let (session, outcome) = {
            let mut sink = |event: AdapterEvent| {
                seen.push(event.event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            let turn = Turn {
                workdir: dir,
                text: "hello",
                resume: None,
                mode: ThreadMode::Build,
            };
            let outcome = run_turn(path.as_os_str(), &turn, &mut emitter, None);
            (emitter.into_native_session(), outcome)
        };
        (seen, session, outcome)
    }

    #[test]
    fn streams_events_and_tracks_the_session_until_the_result() {
        let fake = FakeProgram::new(
            r#"read line
echo '{"type":"system","session_id":"s-1"}'
echo 'not json'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}'
echo '{"type":"result","usage":{"input_tokens":1,"output_tokens":1}}'
cat >/dev/null"#,
        );
        let (seen, session, outcome) = run(&fake.path, &fake.dir);
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
        assert_eq!(session, Some(Value::String("s-1".into())));
    }

    #[test]
    fn a_failing_exit_before_the_result_becomes_an_error_event() {
        let fake = FakeProgram::new("read line\nexit 3");
        let (seen, _, outcome) = run(&fake.path, &fake.dir);
        outcome.unwrap();
        assert_eq!(seen, [kinds::ERROR]);
    }

    #[test]
    fn the_prompt_is_sent_as_a_stream_json_user_message() {
        let fake = FakeProgram::new(
            r#"read line
printf '%s' "$line" > "$(dirname "$0")/received"
echo '{"type":"result"}'"#,
        );
        let mut sink = |_: AdapterEvent| Ok(());
        let mut emitter = Emitter::new(&mut sink);
        let turn = Turn {
            workdir: &fake.dir,
            text: "hello",
            resume: Some("s-9"),
            mode: ThreadMode::Plan,
        };
        run_turn(fake.path.as_os_str(), &turn, &mut emitter, None).unwrap();
        let received = std::fs::read_to_string(fake.dir.join("received")).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&received).unwrap(),
            user_message("hello")
        );
    }

    #[test]
    fn arguments_pick_the_permission_mode_and_resume_session() {
        let turn = Turn {
            workdir: Path::new("/w"),
            text: "t",
            resume: Some("abc"),
            mode: ThreadMode::Plan,
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
            resume: None,
            ..turn
        };
        assert!(arguments(&build).contains(&"bypassPermissions".to_string()));
    }
}
