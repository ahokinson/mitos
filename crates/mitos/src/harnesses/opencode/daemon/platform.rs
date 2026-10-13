use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use super::log_path;

pub(super) fn start_detached(root: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("could not find the mitos executable")?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path(root))
        .with_context(|| format!("could not open {}", log_path(root).display()))?;
    let mut command = Command::new(exe);
    command
        .arg("daemon")
        .arg("run")
        .env("MITOS_STATE_DIR", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map(|_| ())
        .context("could not start the mitos daemon")
}

/// Exclusive when held by the daemon; `None` when another daemon holds it.
pub(super) fn try_lock(root: &Path) -> Result<Option<Lock>> {
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(super::lock_path(root))
        .with_context(|| format!("cannot open {}", super::lock_path(root).display()))?;
    #[cfg(unix)]
    {
        use nix::fcntl::{Flock, FlockArg};
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(flock) => Ok(Some(Lock { _file: flock })),
            Err((_, nix::errno::Errno::EAGAIN)) => Ok(None),
            Err((_, error)) => Err(anyhow::anyhow!(
                "cannot lock {}: {error}",
                super::lock_path(root).display()
            )),
        }
    }
    #[cfg(windows)]
    {
        drop(file);
        Ok(Some(Lock::Windows))
    }
}

pub(super) struct Lock {
    #[cfg(unix)]
    _file: nix::fcntl::Flock<fs::File>,
    #[cfg(windows)]
    _held: (),
}

pub(super) fn watch_signals() -> Option<signal_hook::iterator::Signals> {
    signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT])
        .ok()
}

/// A PID worth signalling. `kill` treats 0 as "my process group" and -1 as
/// "every process I can reach", so those must never escape as arguments.
#[cfg(unix)]
fn pid_of(pid: u32) -> Option<nix::unistd::Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|raw| *raw > 0)
        .map(nix::unistd::Pid::from_raw)
}

#[cfg(unix)]
pub(super) fn process_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    pid_of(pid).is_some_and(|pid| kill(pid, None).is_ok())
}

#[cfg(unix)]
pub(super) fn signal_process(pid: u32) {
    use nix::sys::signal::{Signal, kill};
    if let Some(pid) = pid_of(pid) {
        let _ = kill(pid, Some(Signal::SIGTERM));
    }
}

#[cfg(unix)]
pub(super) fn kill_process(pid: u32) {
    use nix::sys::signal::{Signal, kill};
    if let Some(pid) = pid_of(pid) {
        let _ = kill(pid, Some(Signal::SIGKILL));
    }
}

#[cfg(windows)]
pub(super) fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
pub(super) fn signal_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .status();
}

#[cfg(windows)]
pub(super) fn kill_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .status();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// `kill(-1)` would end the whole user session and `kill(0)` the process
    /// group; neither may escape a u32 that wraps or zeroes on conversion.
    #[test]
    fn pids_that_would_signal_everything_are_ignored() {
        assert!(pid_of(0).is_none());
        assert!(pid_of(u32::MAX).is_none());
        assert!(pid_of(1).is_some());
        kill_process(0);
        kill_process(u32::MAX);
        signal_process(0);
        signal_process(u32::MAX);
        assert!(!process_alive(0));
        assert!(!process_alive(u32::MAX));
    }

    #[test]
    fn a_second_daemon_cannot_take_the_lock() {
        let root = std::env::temp_dir().join(format!("mitos-platform-{}", crate::domain::id()));
        std::fs::create_dir_all(root.join("locks")).unwrap();

        let held = try_lock(&root).unwrap();
        assert!(held.is_some());
        assert!(try_lock(&root).unwrap().is_none());
        drop(held);
        assert!(try_lock(&root).unwrap().is_some());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn signals_are_watched_and_the_current_process_is_alive() {
        assert!(watch_signals().is_some());
        assert!(process_alive(std::process::id()));
    }
}
