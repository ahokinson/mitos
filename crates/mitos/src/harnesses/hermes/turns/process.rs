use std::ffi::OsStr;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use super::{SharedPending, answer_handler, drive_turn};
use crate::harnesses::{Emitter, Turn};
use crate::ipc::answers::AnswerListener;
use crate::json::string_value;
use crate::rpc::RpcPeer;

const DIAGNOSTIC_WAIT: Duration = Duration::from_millis(200);

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

trait HermesProcess {
    type Stdin: Write + Send + 'static;
    type Stdout: Read + Send + 'static;

    fn take_stdin(&mut self) -> Option<Self::Stdin>;
    fn take_stdout(&mut self) -> Option<Self::Stdout>;
    fn kill(&mut self);
    fn wait(&mut self);
}

impl HermesProcess for Child {
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

    fn wait(&mut self) {
        let _ = self.wait();
    }
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
    let mut stderr = child.stderr.take().context("hermes stderr unavailable")?;
    let (log_sender, log) = mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = log_sender.send(text);
    });

    let outcome = run_with_process(child, turn, emitter, answers, &|| {
        log.recv_timeout(DIAGNOSTIC_WAIT)
            .map(|text| last_error_line(&text))
            .unwrap_or_default()
    });
    if turn.ephemeral
        && let Some(id) = string_value(emitter.native_session())
    {
        delete_session(program, id);
    }
    outcome
}

fn run_with_process<P: HermesProcess>(
    mut child: P,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
    diagnostic: &dyn Fn() -> String,
) -> Result<()> {
    let Some(stdin) = child.take_stdin() else {
        child.kill();
        child.wait();
        anyhow::bail!("hermes stdin unavailable");
    };
    let Some(stdout) = child.take_stdout() else {
        child.kill();
        child.wait();
        anyhow::bail!("hermes stdout unavailable");
    };
    let mut peer = RpcPeer::new(stdin, BufReader::new(stdout), true);
    let writer = peer.writer();
    let pending: SharedPending = Arc::default();
    let outcome = (|| {
        let _listener = answers
            .map(|path| {
                AnswerListener::start(path, answer_handler(writer.clone(), pending.clone()))
            })
            .transpose()?;
        drive_turn(turn, &mut peer, emitter, &pending, diagnostic)
    })();
    writer.close();
    child.kill();
    child.wait();
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::domain::ThreadMode;
    use crate::wire::events::kinds;

    struct FakeProcess {
        stdin: Option<Vec<u8>>,
        stdout: Option<Cursor<Vec<u8>>>,
        killed: Arc<AtomicBool>,
        waited: Arc<AtomicBool>,
    }

    impl FakeProcess {
        fn new(stdin: bool, stdout: Option<&str>) -> Self {
            Self {
                stdin: stdin.then(Vec::new),
                stdout: stdout.map(|text| Cursor::new(text.as_bytes().to_vec())),
                killed: Arc::new(AtomicBool::new(false)),
                waited: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl HermesProcess for FakeProcess {
        type Stdin = Vec<u8>;
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
    fn missing_streams_are_cleaned_up() {
        let process = FakeProcess::new(false, Some(""));
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let mut sink = |_| Ok(());
        let mut emitter = Emitter::new(&mut sink);

        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None, &|| String::new())
                .unwrap_err()
                .to_string(),
            "hermes stdin unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));

        let process = FakeProcess::new(true, None);
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        assert_eq!(
            run_with_process(process, &turn(), &mut emitter, None, &|| String::new())
                .unwrap_err()
                .to_string(),
            "hermes stdout unavailable"
        );
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));
    }

    #[test]
    fn an_in_memory_peer_reports_its_diagnostic_and_cleans_up() {
        let process = FakeProcess::new(true, Some(""));
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);

        run_with_process(process, &turn(), &mut emitter, None, &|| {
            "the ACP stream closed".into()
        })
        .unwrap();

        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, kinds::ERROR);
        assert!(
            events[0]
                .content
                .as_deref()
                .is_some_and(|text| text.contains("the ACP stream closed"))
        );
    }
}
