mod platform;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use self::platform::{
    kill_process, process_alive, signal_process, start_detached, try_lock, watch_signals,
};
pub(crate) use super::clients::ServerAccess;
use super::clients::ServerClient;
use super::servers::spawn_server;
use crate::config::resolve_root;
use crate::harnesses::{Harness, OpenCodeAdapter};

const STATE_FILE: &str = "opencode-server.json";
const LOCK_FILE: &str = "opencode-server.lock";
const LOG_FILE: &str = "opencode-server.log";
const START_WAIT: Duration = Duration::from_secs(15);
const STOP_WAIT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(1);
const DEFAULT_IDLE: Duration = Duration::from_mins(15);

/// One shared `opencode serve` outliving every turn. The state file records
/// where it listens and which processes own it; its mtime is touched before
/// each turn so the daemon can reap an idle server.
#[derive(Serialize, Deserialize)]
pub(crate) struct DaemonState {
    pub url: String,
    pub password: String,
    pub daemon_pid: u32,
    pub server_pid: u32,
    pub started_at: i64,
}

impl DaemonState {
    fn access(&self) -> ServerAccess {
        ServerAccess {
            base: self.url.clone(),
            password: self.password.clone(),
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_SERVER: std::cell::RefCell<Option<ServerAccess>> =
        const { std::cell::RefCell::new(None) };
}

/// A turn's entry point: the shared server when one is healthy, `None` when
/// the turn must start a private one. Never blocks a turn on daemon trouble;
/// a daemon that cannot start in time is simply not used.
pub(super) fn ensure_server(workdir: &Path) -> Option<ServerAccess> {
    #[cfg(test)]
    {
        // Thread-local: the parallel run_turn tests stay on the private path.
        let override_ = TEST_SERVER.with(|slot| slot.borrow().clone());
        if let Some(access) = override_ {
            return Some(access);
        }
    }
    if shared_disabled() {
        return None;
    }
    let root = resolve_root(std::env::var_os("MITOS_STATE_DIR").map(PathBuf::from));
    ensure_in(
        &root,
        &workdir.to_string_lossy(),
        &start_detached,
        START_WAIT,
    )
}

fn shared_disabled() -> bool {
    matches!(
        std::env::var("MITOS_OPENCODE_SERVER").as_deref(),
        Ok("per-turn")
    )
}

/// The shared server for one-shot control operations, which have no per-turn
/// fallback: abort, fork, share and diff genuinely need it.
pub(super) fn control_access(workdir: &Path) -> anyhow::Result<ServerAccess> {
    #[cfg(test)]
    {
        let override_ = TEST_SERVER.with(|slot| slot.borrow().clone());
        if let Some(access) = override_ {
            return Ok(access);
        }
    }
    if shared_disabled() {
        anyhow::bail!("the shared OpenCode server is disabled by MITOS_OPENCODE_SERVER=per-turn");
    }
    let root = resolve_root(std::env::var_os("MITOS_STATE_DIR").map(PathBuf::from));
    ensure_in(
        &root,
        &workdir.to_string_lossy(),
        &start_detached,
        START_WAIT,
    )
    .context("the shared OpenCode server is unavailable")
}

fn ensure_in(
    root: &Path,
    workdir: &str,
    starter: &dyn Fn(&Path) -> Result<()>,
    wait: Duration,
) -> Option<ServerAccess> {
    if let Some(state) = read_state(root) {
        if state_healthy(&state, workdir) {
            touch_state(root);
            return Some(state.access());
        }
        remove_stale(root, &state);
    }
    if starter(root).is_err() {
        return None;
    }
    wait_for_healthy(root, workdir, wait)
}

fn wait_for_healthy(root: &Path, workdir: &str, wait: Duration) -> Option<ServerAccess> {
    let deadline = SystemTime::now() + wait;
    loop {
        if let Some(state) = read_state(root)
            && state_healthy(&state, workdir)
        {
            return Some(state.access());
        }
        if SystemTime::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn state_healthy(state: &DaemonState, workdir: &str) -> bool {
    ServerClient::new(&state.access(), workdir).healthy()
}

/// The daemon died without cleaning up: stop the server it left behind and
/// forget the state so the next attempt starts fresh.
fn remove_stale(root: &Path, state: &DaemonState) {
    kill_process(state.server_pid);
    let _ = fs::remove_file(state_path(root));
}

fn state_path(root: &Path) -> PathBuf {
    root.join(STATE_FILE)
}

fn lock_path(root: &Path) -> PathBuf {
    root.join("locks").join(LOCK_FILE)
}

fn log_path(root: &Path) -> PathBuf {
    root.join(LOG_FILE)
}

fn read_state(root: &Path) -> Option<DaemonState> {
    let text = fs::read_to_string(state_path(root)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_state(root: &Path, state: &DaemonState) -> Result<()> {
    let path = state_path(root);
    let temporary = path.with_file_name(format!("{STATE_FILE}.mitos-tmp"));
    fs::write(&temporary, serde_json::to_string(state)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, &path)?;
    Ok(())
}

/// Marks the state as used, resetting the daemon's idle clock.
fn touch_state(root: &Path) {
    if let Ok(file) = fs::File::options().write(true).open(state_path(root)) {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn idle_limit() -> Duration {
    std::env::var("MITOS_OPENCODE_IDLE_SECS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .map_or(DEFAULT_IDLE, Duration::from_secs)
}

fn idle_elapsed(root: &Path, limit: Duration) -> bool {
    let Ok(modified) = fs::metadata(state_path(root)).and_then(|meta| meta.modified()) else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|since| since >= limit)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

fn daemon_root() -> PathBuf {
    resolve_root(std::env::var_os("MITOS_STATE_DIR").map(PathBuf::from))
}

/// `mitos daemon start`: make sure a healthy shared server exists.
pub(crate) fn start() -> Result<()> {
    let root = daemon_root();
    fs::create_dir_all(root.join("locks")).context("could not create the mitos state directory")?;
    if let Some(state) = read_state(&root)
        && state_healthy(&state, &root.to_string_lossy())
    {
        println!(
            "already running at {} (pid {})",
            state.url, state.daemon_pid
        );
        return Ok(());
    }
    start_detached(&root)?;
    match wait_for_healthy(&root, &root.to_string_lossy(), START_WAIT) {
        Some(access) => {
            println!("started the shared OpenCode server at {}", access.base);
            Ok(())
        }
        None => anyhow::bail!(
            "the shared OpenCode server did not become healthy in time; see {}",
            log_path(&root).display()
        ),
    }
}

/// `mitos daemon stop`: ask the daemon to stop, escalating if it will not.
pub(crate) fn stop() {
    let root = daemon_root();
    let Some(state) = read_state(&root) else {
        println!("the shared OpenCode server is not running");
        return;
    };
    if !process_alive(state.daemon_pid) {
        kill_process(state.server_pid);
        let _ = fs::remove_file(state_path(&root));
        println!("cleaned up a leftover OpenCode server");
        return;
    }
    signal_process(state.daemon_pid);
    let deadline = SystemTime::now() + STOP_WAIT;
    while SystemTime::now() < deadline {
        if !state_path(&root).exists() {
            println!("stopped the shared OpenCode server at {}", state.url);
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    kill_process(state.daemon_pid);
    kill_process(state.server_pid);
    let _ = fs::remove_file(state_path(&root));
    println!("forced the shared OpenCode server to stop");
}

/// `mitos daemon status`.
pub(crate) fn status(json: bool) {
    let root = daemon_root();
    let running = read_state(&root).filter(|state| state_healthy(state, &root.to_string_lossy()));
    let report = match &running {
        Some(state) => json!({
            "running": true,
            "url": state.url,
            "daemon_pid": state.daemon_pid,
            "server_pid": state.server_pid,
            "started_at": state.started_at,
        }),
        None => json!({ "running": false }),
    };
    if json {
        println!("{report}");
    } else {
        match &running {
            Some(state) => println!(
                "running at {} (daemon pid {}, server pid {})",
                state.url, state.daemon_pid, state.server_pid
            ),
            None => println!("not running"),
        }
    }
}

/// The daemon process itself: owns the lock, the server, and the state file.
pub(crate) fn run() -> Result<()> {
    let root = daemon_root();
    fs::create_dir_all(root.join("locks")).context("could not create the mitos state directory")?;
    let Some(_lock) = try_lock(&root)? else {
        // Another daemon won the lock between the starter's check and here.
        return Ok(());
    };
    let mut spawned = spawn_server(OsStr::new(OpenCodeAdapter::PROGRAM), &root)?;
    let state = DaemonState {
        url: spawned.access.base.clone(),
        password: spawned.access.password.clone(),
        daemon_pid: std::process::id(),
        server_pid: spawned.child.id(),
        started_at: now_secs(),
    };
    write_state(&root, &state)?;
    let mut signals = watch_signals();
    #[cfg(windows)]
    let _ = &signals;
    let idle = idle_limit();
    loop {
        #[cfg(unix)]
        if let Some(signals) = signals.as_mut()
            && signals.pending().next().is_some()
        {
            break;
        }
        if spawned.child.try_wait()?.is_some() {
            break;
        }
        if !state_path(&root).exists() {
            break;
        }
        if idle_elapsed(&root, idle) {
            break;
        }
        thread::sleep(TICK);
    }
    let _ = spawned.child.kill();
    let _ = spawned.child.wait();
    let _ = fs::remove_file(state_path(&root));
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_test_server(access: Option<ServerAccess>) {
    TEST_SERVER.with(|slot| *slot.borrow_mut() = access);
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::*;

    struct Scratch {
        root: PathBuf,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn scratch() -> Scratch {
        let root = std::env::temp_dir().join(format!("mitos-daemon-{}", crate::domain::id()));
        fs::create_dir_all(root.join("locks")).unwrap();
        Scratch { root }
    }

    /// A `/global/health` responder that stays up until dropped by address.
    fn standing_health_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let mut stream = stream;
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                );
                let _ = stream.flush();
            }
        });
        url
    }

    fn healthy_state(url: &str) -> DaemonState {
        DaemonState {
            url: url.into(),
            password: "pw".into(),
            daemon_pid: u32::MAX,
            server_pid: u32::MAX,
            started_at: 0,
        }
    }

    #[test]
    fn the_state_round_trips_through_the_file() {
        let root = scratch();
        let state = healthy_state("http://127.0.0.1:9");
        write_state(&root.root, &state).unwrap();
        assert_eq!(read_state(&root.root).unwrap().url, state.url);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(state_path(&root.root))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_corrupt_state_file_reads_as_none() {
        let root = scratch();
        fs::write(state_path(&root.root), "not json").unwrap();
        assert!(read_state(&root.root).is_none());
    }

    #[test]
    fn a_healthy_state_is_reused_and_touched() {
        let root = scratch();
        let url = standing_health_server();
        write_state(&root.root, &healthy_state(&url)).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        let file = fs::File::options()
            .write(true)
            .open(state_path(&root.root))
            .unwrap();
        file.set_modified(old).unwrap();
        drop(file);

        let access = ensure_in(
            &root.root,
            "/work",
            &|_| panic!("the starter must not run when the server is healthy"),
            Duration::from_millis(100),
        );

        assert_eq!(access.unwrap().base, url);
        let modified = fs::metadata(state_path(&root.root))
            .unwrap()
            .modified()
            .unwrap();
        assert!(modified > old);
    }

    #[test]
    fn an_unhealthy_state_is_removed_and_the_starter_runs() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://127.0.0.1:1")).unwrap();
        let url = standing_health_server();

        let access = ensure_in(
            &root.root,
            "/work",
            &|root| write_state(root, &healthy_state(&url)),
            Duration::from_secs(2),
        );

        assert_eq!(access.unwrap().base, url);
    }

    #[test]
    fn a_failing_start_yields_none_and_leaves_no_state() {
        let root = scratch();
        let access = ensure_in(
            &root.root,
            "/work",
            &|_| anyhow::bail!("no daemon"),
            Duration::from_millis(100),
        );
        assert!(access.is_none());
        assert!(!state_path(&root.root).exists());
    }

    #[test]
    fn a_start_that_never_becomes_healthy_yields_none() {
        let root = scratch();
        let access = ensure_in(&root.root, "/work", &|_| Ok(()), Duration::from_millis(300));
        assert!(access.is_none());
    }

    #[test]
    fn idle_counts_from_the_state_mtime() {
        let root = scratch();
        assert!(idle_elapsed(&root.root, Duration::from_secs(0)));
        write_state(&root.root, &healthy_state("http://127.0.0.1:1")).unwrap();
        assert!(!idle_elapsed(&root.root, Duration::from_secs(3600)));
        let file = fs::File::options()
            .write(true)
            .open(state_path(&root.root))
            .unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(10))
            .unwrap();
        drop(file);
        assert!(idle_elapsed(&root.root, Duration::from_secs(5)));
        assert!(!idle_elapsed(&root.root, Duration::from_secs(60)));
    }

    #[test]
    fn idle_limit_reads_the_env_else_the_default() {
        // SAFETY: no other test reads MITOS_OPENCODE_IDLE_SECS.
        unsafe { std::env::set_var("MITOS_OPENCODE_IDLE_SECS", "7") };
        assert_eq!(idle_limit(), Duration::from_secs(7));
        // SAFETY: restored before anything else can observe the swap.
        unsafe { std::env::remove_var("MITOS_OPENCODE_IDLE_SECS") };
        assert_eq!(idle_limit(), DEFAULT_IDLE);
    }

    #[test]
    fn now_secs_counts_since_the_epoch() {
        assert!(now_secs() > 1_700_000_000);
    }

    #[test]
    fn the_daemon_commands_read_state_and_manage_it() {
        let root = scratch();
        let url = standing_health_server();
        write_state(&root.root, &healthy_state(&url)).unwrap();
        // SAFETY: only this test reads MITOS_STATE_DIR while it runs.
        unsafe { std::env::set_var("MITOS_STATE_DIR", &root.root) };

        assert_eq!(daemon_root(), root.root);
        assert!(start().is_ok());
        status(true);
        status(false);
        assert!(state_path(&root.root).exists());

        stop();
        assert!(!state_path(&root.root).exists());
        status(false);

        // SAFETY: restored before anything else can observe the swap.
        unsafe { std::env::remove_var("MITOS_STATE_DIR") };
    }
}
