use std::cell::Cell;
use std::ffi::OsStr;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};

use super::super::tokens::TurnTokens;
use super::{Session, SharedPending, answer_handler, drive};
use crate::harnesses::{Emitter, Turn};
use crate::ipc::answers::AnswerListener;
use crate::rpc::RpcPeer;

fn read_stderr(handle: JoinHandle<String>) -> String {
    handle.join().unwrap_or_default().trim().to_string()
}

trait CodexProcess {
    type Stdin: Write + Send + 'static;
    type Stdout: Read + Send + 'static;

    fn take_stdin(&mut self) -> Option<Self::Stdin>;
    fn take_stdout(&mut self) -> Option<Self::Stdout>;
    fn has_exited(&mut self) -> bool;
    fn kill(&mut self);
    fn wait(&mut self);
}

impl CodexProcess for Child {
    type Stdin = ChildStdin;
    type Stdout = std::process::ChildStdout;

    fn take_stdin(&mut self) -> Option<Self::Stdin> {
        self.stdin.take()
    }

    fn take_stdout(&mut self) -> Option<Self::Stdout> {
        self.stdout.take()
    }

    fn has_exited(&mut self) -> bool {
        self.try_wait().ok().flatten().is_some()
    }

    fn kill(&mut self) {
        let _ = self.kill();
    }

    fn wait(&mut self) {
        let _ = self.wait();
    }
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
    let mut stderr = child.stderr.take().context("codex stderr unavailable")?;
    let diagnostics = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    run_with_process(child, turn, emitter, answers, || read_stderr(diagnostics))
}

fn run_with_process<P: CodexProcess>(
    mut child: P,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
    diagnostics: impl FnOnce() -> String,
) -> Result<()> {
    let Some(stdin) = child.take_stdin() else {
        child.kill();
        child.wait();
        bail!("codex stdin unavailable");
    };
    let Some(stdout) = child.take_stdout() else {
        child.kill();
        child.wait();
        bail!("codex stdout unavailable");
    };

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

    let complete = finished.get();
    drop(session);
    let already_exited = child.has_exited();
    let mut diagnostics = Some(diagnostics);
    let mut read_diagnostics = || {
        diagnostics
            .take()
            .expect("codex diagnostics read more than once")()
    };
    let reported = match outcome {
        Ok(()) => {
            let diagnostic = if complete {
                String::new()
            } else {
                read_diagnostics()
            };
            super::finish_turn(Ok(()), complete, diagnostic, emitter)
        }
        Err(cause) => {
            let diagnostic = if already_exited {
                read_diagnostics()
            } else {
                String::new()
            };
            super::finish_turn(Err(cause), false, diagnostic, emitter)
        }
    };
    writer.close();
    child.kill();
    child.wait();
    reported
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::domain::ThreadMode;
    use crate::wire::events::kinds;

    struct FakeProcess {
        stdin: Option<Vec<u8>>,
        stdout: Option<Cursor<Vec<u8>>>,
        exited: bool,
        killed: Arc<AtomicBool>,
        waited: Arc<AtomicBool>,
    }

    impl FakeProcess {
        fn new(stdin: bool, stdout: Option<&str>, exited: bool) -> Self {
            Self {
                stdin: stdin.then(Vec::new),
                stdout: stdout.map(|text| Cursor::new(text.as_bytes().to_vec())),
                exited,
                killed: Arc::new(AtomicBool::new(false)),
                waited: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl CodexProcess for FakeProcess {
        type Stdin = Vec<u8>;
        type Stdout = Cursor<Vec<u8>>;

        fn take_stdin(&mut self) -> Option<Self::Stdin> {
            self.stdin.take()
        }

        fn take_stdout(&mut self) -> Option<Self::Stdout> {
            self.stdout.take()
        }

        fn has_exited(&mut self) -> bool {
            self.exited
        }

        fn kill(&mut self) {
            self.killed.store(true, Ordering::Relaxed);
        }

        fn wait(&mut self) {
            self.waited.store(true, Ordering::Relaxed);
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
    fn missing_streams_are_cleaned_up_without_starting_a_process() {
        let process = FakeProcess::new(false, Some(""), false);
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let mut sink = |_| Ok(());
        let mut emitter = Emitter::new(&mut sink);

        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None, String::new)
                .unwrap_err()
                .to_string(),
            "codex stdin unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));

        let process = FakeProcess::new(true, None, false);
        let killed = process.killed.clone();
        let waited = process.waited.clone();

        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None, String::new)
                .unwrap_err()
                .to_string(),
            "codex stdout unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));
    }

    #[test]
    fn an_in_memory_peer_reports_its_protocol_failure_and_cleans_up() {
        let process = FakeProcess::new(true, Some(""), true);
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let diagnostics = Arc::new(AtomicBool::new(false));
        let read_diagnostics = diagnostics.clone();
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);

        run_with_process(process, &turn(), &mut emitter, None, move || {
            read_diagnostics.store(true, Ordering::Relaxed);
            "the app server closed".into()
        })
        .unwrap();

        assert!(diagnostics.load(Ordering::Relaxed));
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, kinds::ERROR);
        assert!(
            events[0]
                .content
                .as_deref()
                .is_some_and(|text| text.contains("the app server closed"))
        );
    }
}
