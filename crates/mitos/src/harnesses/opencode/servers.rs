use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use thiserror::Error;

use super::clients::{SERVER_USER, ServerAccess};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const LISTENING: &str = "listening on ";

/// A private `opencode serve` the caller owns; the caller kills `child`.
pub struct SpawnedServer {
    pub child: Child,
    pub access: ServerAccess,
}

trait ServerProcess {
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>>;
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>>;
    fn exit_code(&mut self) -> Option<i32>;
    fn has_exited(&mut self) -> bool;
    fn kill(&mut self);
    fn wait(&mut self);
}

impl ServerProcess for Child {
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stdout.take().map(|stream| Box::new(stream) as _)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.stderr.take().map(|stream| Box::new(stream) as _)
    }

    fn exit_code(&mut self) -> Option<i32> {
        self.wait().ok().and_then(|status| status.code())
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

/// Why a server could not be started. `Spawn` means the harness binary itself
/// could not run; `Startup` means it ran but never became usable.
#[derive(Debug, Error)]
pub(super) enum ServerStartError {
    #[error(transparent)]
    Spawn(#[from] anyhow::Error),
    #[error("{0}")]
    Startup(String),
}

/// Starts `opencode serve` on a random loopback port behind a fresh password
/// and waits for it to announce where it is listening. On failure the child is
/// cleaned up and the error carries the exit code or stderr diagnostics.
pub(super) fn spawn_server(
    program: &OsStr,
    workdir: &Path,
) -> std::result::Result<SpawnedServer, ServerStartError> {
    let password = uuid::Uuid::new_v4().to_string();
    let child = Command::new(program)
        .args(["serve", "--port", "0", "--hostname", "127.0.0.1"])
        .current_dir(workdir)
        .env("OPENCODE_SERVER_USERNAME", SERVER_USER)
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start opencode")
        .map_err(ServerStartError::Spawn)?;
    let (child, access) = start_with_process(child, password)?;
    Ok(SpawnedServer { child, access })
}

fn start_with_process<P: ServerProcess>(
    mut child: P,
    password: String,
) -> std::result::Result<(P, ServerAccess), ServerStartError> {
    let Some(stdout) = child.take_stdout() else {
        child.kill();
        child.wait();
        return Err(ServerStartError::Spawn(anyhow::anyhow!(
            "opencode stdout unavailable"
        )));
    };
    let Some(stderr) = child.take_stderr() else {
        child.kill();
        child.wait();
        return Err(ServerStartError::Spawn(anyhow::anyhow!(
            "opencode stderr unavailable"
        )));
    };
    let diagnostics = read_all(stderr);
    let urls = forward_listening_url(stdout);
    finish_startup(
        child,
        password,
        urls.recv_timeout(STARTUP_TIMEOUT),
        diagnostics,
    )
}

fn finish_startup<P: ServerProcess>(
    mut child: P,
    password: String,
    received: std::result::Result<String, mpsc::RecvTimeoutError>,
    diagnostics: JoinHandle<String>,
) -> std::result::Result<(P, ServerAccess), ServerStartError> {
    let started = match received {
        Ok(base) => Ok(ServerAccess { base, password }),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("opencode serve did not start in time".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(child.exit_code().map_or_else(
            || "opencode serve exited before listening".into(),
            |code| format!("opencode serve exited with code {code}"),
        )),
    };

    match started {
        Ok(access) => Ok((child, access)),
        Err(cause) => {
            let already_exited = child.has_exited();
            let diagnostic = if already_exited {
                diagnostics.join().unwrap_or_default().trim().to_string()
            } else {
                String::new()
            };
            child.kill();
            child.wait();
            let message = if diagnostic.is_empty() {
                cause
            } else {
                diagnostic
            };
            Err(ServerStartError::Startup(message))
        }
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    use super::*;

    struct FakeProcess {
        stdout: Option<Cursor<Vec<u8>>>,
        stderr: Option<Cursor<Vec<u8>>>,
        exit_code: Option<i32>,
        exited: bool,
        killed: Arc<AtomicBool>,
        waited: Arc<AtomicBool>,
    }

    impl FakeProcess {
        fn new(stdout: Option<&str>, stderr: Option<&str>) -> Self {
            Self {
                stdout: stdout.map(|text| Cursor::new(text.as_bytes().to_vec())),
                stderr: stderr.map(|text| Cursor::new(text.as_bytes().to_vec())),
                exit_code: None,
                exited: false,
                killed: Arc::new(AtomicBool::new(false)),
                waited: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl ServerProcess for FakeProcess {
        fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
            self.stdout.take().map(|stream| Box::new(stream) as _)
        }

        fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
            self.stderr.take().map(|stream| Box::new(stream) as _)
        }

        fn exit_code(&mut self) -> Option<i32> {
            self.exit_code
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

    #[test]
    fn stdout_forwarding_announces_only_the_first_listening_url() {
        let urls = forward_listening_url(Cursor::new(
            "starting\nlistening on ftp://ignored\nlistening on http://127.0.0.1:4096\nlistening on http://127.0.0.1:4097\n",
        ));

        assert_eq!(urls.recv().unwrap(), "http://127.0.0.1:4096");
        assert!(urls.try_recv().is_err());
    }

    #[test]
    fn diagnostics_are_drained_in_the_background() {
        assert_eq!(
            read_all(Cursor::new("first\nsecond\n")).join().unwrap(),
            "first\nsecond\n"
        );
    }

    #[test]
    fn an_announced_server_returns_its_access_without_stopping_it() {
        let process = FakeProcess::new(Some("listening on http://127.0.0.1:4096\n"), Some(""));
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let password = uuid::Uuid::new_v4().to_string();

        let (_process, access) = start_with_process(process, password.clone()).unwrap();

        assert_eq!(access.base, "http://127.0.0.1:4096");
        assert_eq!(access.password, password);
        assert!(!killed.load(Ordering::Relaxed));
        assert!(!waited.load(Ordering::Relaxed));
    }

    #[test]
    fn missing_output_streams_stop_the_server() {
        for process in [
            FakeProcess::new(None, Some("")),
            FakeProcess::new(Some(""), None),
        ] {
            let killed = process.killed.clone();
            let waited = process.waited.clone();

            assert!(start_with_process(process, uuid::Uuid::new_v4().to_string()).is_err());
            assert!(killed.load(Ordering::Relaxed));
            assert!(waited.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn failed_startup_uses_diagnostics_or_the_exit_code_then_stops_the_server() {
        let process = FakeProcess {
            exit_code: Some(23),
            exited: true,
            ..FakeProcess::new(None, None)
        };
        let killed = process.killed.clone();
        let waited = process.waited.clone();
        let result = finish_startup(
            process,
            uuid::Uuid::new_v4().to_string(),
            Err(mpsc::RecvTimeoutError::Disconnected),
            thread::spawn(|| "fatal configuration error".into()),
        );

        let Err(error) = result else {
            panic!("the failed server must return an error");
        };
        assert_eq!(error.to_string(), "fatal configuration error");
        assert!(killed.load(Ordering::Relaxed));
        assert!(waited.load(Ordering::Relaxed));

        let process = FakeProcess::new(None, None);
        let result = finish_startup(
            process,
            uuid::Uuid::new_v4().to_string(),
            Err(mpsc::RecvTimeoutError::Timeout),
            thread::spawn(String::new),
        );
        let Err(error) = result else {
            panic!("the timed-out server must return an error");
        };
        assert_eq!(error.to_string(), "opencode serve did not start in time");
    }
}
