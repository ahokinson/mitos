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
    shared_disabled_from(std::env::var("MITOS_OPENCODE_SERVER").ok().as_deref())
}

fn shared_disabled_from(value: Option<&str>) -> bool {
    value == Some("per-turn")
}

/// The shared server for one-shot control operations, which have no per-turn
/// fallback: abort, fork, share and diff genuinely need it.
pub(super) fn control_access(workdir: &Path) -> anyhow::Result<ServerAccess> {
    #[cfg(test)]
    let override_ = TEST_SERVER.with(|slot| slot.borrow().clone());
    #[cfg(not(test))]
    let override_ = None;
    control_access_with(override_, shared_disabled(), &|| {
        let root = resolve_root(std::env::var_os("MITOS_STATE_DIR").map(PathBuf::from));
        ensure_in(
            &root,
            &workdir.to_string_lossy(),
            &start_detached,
            START_WAIT,
        )
    })
}

fn control_access_with(
    override_: Option<ServerAccess>,
    disabled: bool,
    ensure: &dyn Fn() -> Option<ServerAccess>,
) -> anyhow::Result<ServerAccess> {
    if let Some(access) = override_ {
        return Ok(access);
    }
    if disabled {
        anyhow::bail!("the shared OpenCode server is disabled by MITOS_OPENCODE_SERVER=per-turn");
    }
    ensure().context("the shared OpenCode server is unavailable")
}

fn ensure_in(
    root: &Path,
    workdir: &str,
    starter: &dyn Fn(&Path) -> Result<()>,
    wait: Duration,
) -> Option<ServerAccess> {
    ensure_in_with(root, workdir, starter, &state_healthy, &remove_stale, wait)
}

fn ensure_in_with(
    root: &Path,
    workdir: &str,
    starter: &dyn Fn(&Path) -> Result<()>,
    healthy: &dyn Fn(&DaemonState, &str) -> bool,
    remove_stale_state: &dyn Fn(&Path, &DaemonState),
    wait: Duration,
) -> Option<ServerAccess> {
    if let Some(state) = read_state(root) {
        if healthy(&state, workdir) {
            touch_state(root);
            return Some(state.access());
        }
        remove_stale_state(root, &state);
    }
    if starter(root).is_err() {
        return None;
    }
    wait_for_healthy_with(root, workdir, healthy, wait)
}

fn wait_for_healthy(root: &Path, workdir: &str, wait: Duration) -> Option<ServerAccess> {
    wait_for_healthy_with(root, workdir, &state_healthy, wait)
}

fn wait_for_healthy_with(
    root: &Path,
    workdir: &str,
    healthy: &dyn Fn(&DaemonState, &str) -> bool,
    wait: Duration,
) -> Option<ServerAccess> {
    let deadline = SystemTime::now() + wait;
    loop {
        if let Some(state) = read_state(root)
            && healthy(&state, workdir)
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
    remove_stale_with(root, state, &kill_process);
}

fn remove_stale_with(root: &Path, state: &DaemonState, kill: &dyn Fn(u32)) {
    kill(state.server_pid);
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
    idle_limit_from(std::env::var("MITOS_OPENCODE_IDLE_SECS").ok().as_deref())
}

fn idle_limit_from(value: Option<&str>) -> Duration {
    value
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

enum StartOutcome {
    AlreadyRunning(DaemonState),
    Started(ServerAccess),
    TimedOut,
}

/// `mitos daemon start`: make sure a healthy shared server exists.
pub(crate) fn start() -> Result<()> {
    let root = daemon_root();
    fs::create_dir_all(root.join("locks")).context("could not create the mitos state directory")?;
    let workdir = root.to_string_lossy();
    match start_in(
        &root,
        &workdir,
        &state_healthy,
        &start_detached,
        &wait_for_healthy,
    )? {
        StartOutcome::AlreadyRunning(state) => {
            println!(
                "already running at {} (pid {})",
                state.url, state.daemon_pid
            );
            Ok(())
        }
        StartOutcome::Started(access) => {
            println!("started the shared OpenCode server at {}", access.base);
            Ok(())
        }
        StartOutcome::TimedOut => anyhow::bail!(
            "the shared OpenCode server did not become healthy in time; see {}",
            log_path(&root).display()
        ),
    }
}

fn start_in(
    root: &Path,
    workdir: &str,
    healthy: &dyn Fn(&DaemonState, &str) -> bool,
    starter: &dyn Fn(&Path) -> Result<()>,
    waiter: &dyn Fn(&Path, &str, Duration) -> Option<ServerAccess>,
) -> Result<StartOutcome> {
    if let Some(state) = read_state(root)
        && healthy(&state, workdir)
    {
        return Ok(StartOutcome::AlreadyRunning(state));
    }
    starter(root)?;
    Ok(match waiter(root, workdir, START_WAIT) {
        Some(access) => StartOutcome::Started(access),
        None => StartOutcome::TimedOut,
    })
}

/// `mitos daemon stop`: ask the daemon to stop, escalating if it will not.
pub(crate) fn stop() {
    let root = daemon_root();
    stop_in(
        &root,
        &process_alive,
        &signal_process,
        &kill_process,
        STOP_WAIT,
    );
}

fn stop_in(
    root: &Path,
    is_alive: &dyn Fn(u32) -> bool,
    request_stop: &dyn Fn(u32),
    force_stop: &dyn Fn(u32),
    wait: Duration,
) {
    let Some(state) = read_state(root) else {
        println!("the shared OpenCode server is not running");
        return;
    };
    if !is_alive(state.daemon_pid) {
        force_stop(state.server_pid);
        let _ = fs::remove_file(state_path(root));
        println!("cleaned up a leftover OpenCode server");
        return;
    }
    request_stop(state.daemon_pid);
    let deadline = SystemTime::now() + wait;
    while SystemTime::now() < deadline {
        if !state_path(root).exists() {
            println!("stopped the shared OpenCode server at {}", state.url);
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    force_stop(state.daemon_pid);
    force_stop(state.server_pid);
    let _ = fs::remove_file(state_path(root));
    println!("forced the shared OpenCode server to stop");
}

/// `mitos daemon status`.
pub(crate) fn status(json: bool) {
    let root = daemon_root();
    let workdir = root.to_string_lossy();
    let (running, report) = status_report(&root, &|state| state_healthy(state, &workdir));
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

fn status_report(
    root: &Path,
    healthy: &dyn Fn(&DaemonState) -> bool,
) -> (Option<DaemonState>, serde_json::Value) {
    let running = read_state(root).filter(healthy);
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
    (running, report)
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

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
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

    fn healthy_state(url: &str) -> DaemonState {
        DaemonState {
            url: url.into(),
            password: "pw".into(),
            daemon_pid: 41,
            server_pid: 42,
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
        assert!(read_state(&root.root).is_none());
        fs::write(state_path(&root.root), "not json").unwrap();
        assert!(read_state(&root.root).is_none());
    }

    #[test]
    fn state_access_and_paths_use_the_daemon_root() {
        let root = scratch();
        let state = healthy_state("http://healthy");

        assert_eq!(state.access().base, "http://healthy");
        assert_eq!(state.access().password, "pw");
        assert_eq!(state_path(&root.root), root.root.join(STATE_FILE));
        assert_eq!(
            lock_path(&root.root),
            root.root.join("locks").join(LOCK_FILE)
        );
        assert_eq!(log_path(&root.root), root.root.join(LOG_FILE));
    }

    #[test]
    fn status_reports_a_healthy_server_or_a_stopped_one() {
        let root = scratch();

        let (running, report) = status_report(&root.root, &|_| true);
        assert!(running.is_none());
        assert_eq!(report, json!({ "running": false }));

        let state = healthy_state("http://healthy");
        write_state(&root.root, &state).unwrap();
        let (running, report) = status_report(&root.root, &|state| state.url == "http://healthy");

        assert_eq!(running.unwrap().url, "http://healthy");
        assert_eq!(
            report,
            json!({
                "running": true,
                "url": "http://healthy",
                "daemon_pid": 41,
                "server_pid": 42,
                "started_at": 0,
            })
        );
    }

    #[test]
    fn a_healthy_state_is_reused_and_touched() {
        let root = scratch();
        let url = "http://healthy";
        write_state(&root.root, &healthy_state(url)).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        let file = fs::File::options()
            .write(true)
            .open(state_path(&root.root))
            .unwrap();
        file.set_modified(old).unwrap();
        drop(file);

        let health = |state: &DaemonState, _: &str| state.url == url;
        let access = ensure_in_with(
            &root.root,
            "/work",
            &|_| panic!("the starter must not run when the server is healthy"),
            &health,
            &|_, _| panic!("a healthy state must not be removed"),
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
        write_state(&root.root, &healthy_state("http://unhealthy")).unwrap();
        let url = "http://healthy";
        let stale_removed = Cell::new(false);
        let health = |state: &DaemonState, _: &str| state.url == url;

        let access = ensure_in_with(
            &root.root,
            "/work",
            &|root| write_state(root, &healthy_state(url)),
            &health,
            &|root, _| {
                stale_removed.set(true);
                fs::remove_file(state_path(root)).unwrap();
            },
            Duration::ZERO,
        );

        assert_eq!(access.unwrap().base, url);
        assert!(stale_removed.get());
    }

    #[test]
    fn a_failing_start_yields_none_and_leaves_no_state() {
        let root = scratch();
        let access = ensure_in(
            &root.root,
            "/work",
            &|_| anyhow::bail!("no daemon"),
            Duration::ZERO,
        );
        assert!(access.is_none());
        assert!(!state_path(&root.root).exists());
    }

    #[test]
    fn waiting_without_a_state_yields_none() {
        let root = scratch();
        let access = wait_for_healthy(&root.root, "/work", Duration::ZERO);
        assert!(access.is_none());
    }

    #[test]
    fn shared_server_policy_requires_the_exact_per_turn_value() {
        assert!(shared_disabled_from(Some("per-turn")));
        assert!(!shared_disabled_from(None));
        assert!(!shared_disabled_from(Some("shared")));
        assert!(!shared_disabled_from(Some("per-turns")));
    }

    #[test]
    fn control_access_honors_overrides_and_reports_unavailable_servers() {
        let access = ServerAccess {
            base: "http://healthy".into(),
            password: "pw".into(),
        };
        assert_eq!(
            control_access_with(Some(access), true, &|| panic!("the override must win"))
                .unwrap()
                .base,
            "http://healthy"
        );

        let Err(error) =
            control_access_with(None, true, &|| panic!("disabled servers must not start"))
        else {
            panic!("a disabled shared server must fail");
        };
        assert_eq!(
            error.to_string(),
            "the shared OpenCode server is disabled by MITOS_OPENCODE_SERVER=per-turn"
        );

        let Err(error) = control_access_with(None, false, &|| None) else {
            panic!("an unavailable shared server must fail");
        };
        assert_eq!(
            error.to_string(),
            "the shared OpenCode server is unavailable"
        );
    }

    #[test]
    fn stale_state_cleanup_uses_the_server_pid() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://stale")).unwrap();
        let killed = Cell::new(None);

        remove_stale_with(&root.root, &healthy_state("http://stale"), &|pid| {
            killed.set(Some(pid));
        });

        assert_eq!(killed.get(), Some(42));
        assert!(!state_path(&root.root).exists());
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
    fn idle_limit_parses_the_environment_value_or_uses_the_default() {
        assert_eq!(idle_limit_from(Some("7")), Duration::from_secs(7));
        assert_eq!(idle_limit_from(Some("forever")), DEFAULT_IDLE);
        assert_eq!(idle_limit_from(None), DEFAULT_IDLE);
        assert_eq!(
            idle_limit(),
            idle_limit_from(std::env::var("MITOS_OPENCODE_IDLE_SECS").ok().as_deref())
        );
    }

    #[test]
    fn now_secs_counts_since_the_epoch() {
        assert!(now_secs() > 1_700_000_000);
    }

    #[test]
    fn stop_requests_a_live_daemon_to_exit() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://healthy")).unwrap();
        let requested = Cell::new(None);
        stop_in(
            &root.root,
            &|pid| pid == 41,
            &|pid| {
                requested.set(Some(pid));
                fs::remove_file(state_path(&root.root)).unwrap();
            },
            &|_| panic!("a cooperative daemon must not be force-stopped"),
            Duration::from_secs(1),
        );

        assert_eq!(requested.get(), Some(41));
        assert!(!state_path(&root.root).exists());
    }

    #[test]
    fn stop_does_nothing_when_there_is_no_state() {
        let root = scratch();
        stop_in(
            &root.root,
            &|_| panic!("no process should be inspected without state"),
            &|_| panic!("no process should be stopped without state"),
            &|_| panic!("no process should be killed without state"),
            Duration::ZERO,
        );
    }

    #[test]
    fn start_reuses_a_healthy_state_or_reports_each_startup_outcome() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://healthy")).unwrap();

        let outcome = start_in(
            &root.root,
            "/work",
            &|state, _| state.url == "http://healthy",
            &|_| panic!("a healthy server must not start again"),
            &|_, _, _| panic!("a healthy server must not be awaited"),
        )
        .unwrap();
        let StartOutcome::AlreadyRunning(state) = outcome else {
            panic!("the healthy server must be reused");
        };
        assert_eq!(state.url, "http://healthy");

        let result = start_in(
            &root.root,
            "/work",
            &|_, _| false,
            &|_| anyhow::bail!("cannot detach"),
            &|_, _, _| panic!("a failed starter must not be awaited"),
        );
        let Err(error) = result else {
            panic!("the starter failure must be returned");
        };
        assert_eq!(error.to_string(), "cannot detach");

        let outcome = start_in(
            &root.root,
            "/work",
            &|_, _| false,
            &|_| Ok(()),
            &|_, _, _| {
                Some(ServerAccess {
                    base: "http://started".into(),
                    password: "pw".into(),
                })
            },
        )
        .unwrap();
        let StartOutcome::Started(access) = outcome else {
            panic!("a healthy server must report its access");
        };
        assert_eq!(access.base, "http://started");

        let outcome = start_in(
            &root.root,
            "/work",
            &|_, _| false,
            &|_| Ok(()),
            &|_, _, _| None,
        )
        .unwrap();
        assert!(matches!(outcome, StartOutcome::TimedOut));
    }

    #[test]
    fn stop_cleans_up_a_stale_daemon() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://healthy")).unwrap();
        let forced = Cell::new(None);
        stop_in(
            &root.root,
            &|_| false,
            &|_| panic!("a stale daemon must not receive a stop request"),
            &|pid| forced.set(Some(pid)),
            Duration::ZERO,
        );

        assert_eq!(forced.get(), Some(42));
        assert!(!state_path(&root.root).exists());
    }

    #[test]
    fn stop_force_stops_a_daemon_that_does_not_exit() {
        let root = scratch();
        write_state(&root.root, &healthy_state("http://healthy")).unwrap();
        let forced = RefCell::new(Vec::new());
        stop_in(
            &root.root,
            &|_| true,
            &|_| {},
            &|pid| forced.borrow_mut().push(pid),
            Duration::ZERO,
        );

        assert_eq!(*forced.borrow(), [41, 42]);
        assert!(!state_path(&root.root).exists());
    }
}
