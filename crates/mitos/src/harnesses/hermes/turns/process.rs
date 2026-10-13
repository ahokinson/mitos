use std::ffi::OsStr;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
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
    use super::*;
    use crate::domain::ThreadMode;
    use crate::harnesses::fixtures::{FakeProgram, kinds_of};
    use crate::wire::events::{AdapterEvent, kinds};

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
                agent: None,
                model: None,
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
            agent: None,
            model: None,
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
