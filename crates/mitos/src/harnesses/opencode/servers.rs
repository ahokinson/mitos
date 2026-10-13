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
    let mut child = Command::new(program)
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
    let stdout = child
        .stdout
        .take()
        .context("opencode stdout unavailable")
        .map_err(ServerStartError::Spawn)?;
    let stderr = child
        .stderr
        .take()
        .context("opencode stderr unavailable")
        .map_err(ServerStartError::Spawn)?;
    let diagnostics = read_all(stderr);
    let urls = forward_listening_url(stdout);

    let started = match urls.recv_timeout(STARTUP_TIMEOUT) {
        Ok(base) => Ok(ServerAccess { base, password }),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("opencode serve did not start in time".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(match child.wait() {
            Ok(status) => match status.code() {
                Some(code) => format!("opencode serve exited with code {code}"),
                None => "opencode serve exited before listening".into(),
            },
            Err(_) => "opencode serve exited before listening".into(),
        }),
    };

    match started {
        Ok(access) => Ok(SpawnedServer { child, access }),
        Err(cause) => {
            let already_exited = child.try_wait().ok().flatten().is_some();
            let diagnostic = if already_exited {
                diagnostics.join().unwrap_or_default().trim().to_string()
            } else {
                String::new()
            };
            let _ = child.kill();
            let _ = child.wait();
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
    use super::listening_url;

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
