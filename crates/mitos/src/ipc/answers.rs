use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

const SOCKET_ID_LEN: usize = 12;

/// Unix socket paths are length-limited, so the name uses a thread-id prefix.
pub fn socket_path(state_root: &Path, thread_id: &str) -> PathBuf {
    let short: String = thread_id.chars().take(SOCKET_ID_LEN).collect();
    state_root.join("sockets").join(format!("{short}.sock"))
}

pub struct AnswerListener {
    #[cfg(unix)]
    _inner: unix::Listener,
}

impl AnswerListener {
    #[cfg(unix)]
    pub fn start(
        path: &Path,
        on_answer: impl FnMut(Value) -> Result<()> + Send + 'static,
    ) -> Result<Self> {
        Ok(Self {
            _inner: unix::Listener::start(path, on_answer)?,
        })
    }

    #[cfg(not(unix))]
    pub fn start(
        _path: &Path,
        _on_answer: impl FnMut(Value) -> Result<()> + Send + 'static,
    ) -> Result<Self> {
        anyhow::bail!("answering a running turn requires unix sockets")
    }
}

#[cfg(unix)]
pub fn send_answer(path: &Path, line: &Value) -> Result<()> {
    unix::send(path, line)
}

#[cfg(not(unix))]
pub fn send_answer(_path: &Path, _line: &Value) -> Result<()> {
    anyhow::bail!("answering a running turn requires unix sockets")
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use anyhow::{Context, Result};
    use serde_json::Value;

    const POLL_INTERVAL: Duration = Duration::from_millis(25);
    const IO_TIMEOUT: Duration = Duration::from_secs(2);

    pub struct Listener {
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
        path: PathBuf,
    }

    impl Listener {
        pub fn start(
            path: &Path,
            mut on_answer: impl FnMut(Value) -> Result<()> + Send + 'static,
        ) -> Result<Self> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::remove_file(path);
            let listener = UnixListener::bind(path)
                .with_context(|| format!("could not bind answer socket {}", path.display()))?;
            listener.set_nonblocking(true)?;
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = forward(stream, &mut on_answer);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(POLL_INTERVAL);
                        }
                        Err(_) => break,
                    }
                }
            });
            Ok(Self {
                stop,
                handle: Some(handle),
                path: path.to_path_buf(),
            })
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn forward(stream: UnixStream, on_answer: &mut impl FnMut(Value) -> Result<()>) -> Result<()> {
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let parsed: Value = serde_json::from_str(line.trim())?;
        let mut writer = stream;
        if !parsed.is_object() {
            writer.write_all(b"invalid\n")?;
            return Ok(());
        }
        on_answer(parsed)?;
        writer.write_all(b"ok\n")?;
        Ok(())
    }

    pub fn send(path: &Path, line: &Value) -> Result<()> {
        let mut stream = UnixStream::connect(path)
            .with_context(|| format!("no live turn is listening at {}", path.display()))?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        writeln!(stream, "{line}")?;
        stream.flush()?;
        let mut ack = String::new();
        BufReader::new(stream).read_line(&mut ack)?;
        if ack.trim() != "ok" {
            anyhow::bail!("turn did not accept the answer");
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    use super::*;

    #[test]
    fn socket_path_truncates_the_thread_id() {
        let path = socket_path(Path::new("/state"), "0123456789abcdef");
        assert_eq!(path, Path::new("/state/sockets/0123456789ab.sock"));
    }

    #[test]
    fn answers_reach_the_child_stdin_and_the_socket_is_removed_on_drop() {
        let dir = std::env::temp_dir().join(format!("mitos-answers-{}", crate::domain::id()));
        let path = socket_path(&dir, "thread-xyz-123");
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let listener = AnswerListener::start(&path, move |answer| {
            writeln!(stdin, "{answer}")?;
            stdin.flush()?;
            Ok(())
        })
        .unwrap();

        send_answer(
            &path,
            &serde_json::json!({ "request_id": "r1", "response": "yes" }),
        )
        .unwrap();

        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let echoed: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(echoed["request_id"], "r1");

        drop(listener);
        assert!(!path.exists());
        assert!(send_answer(&path, &serde_json::json!({})).is_err());
        child.wait().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
