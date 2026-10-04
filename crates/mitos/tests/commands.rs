//! Drives the real `mitos` binary against a fake external adapter.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde_json::Value;

/// Scripts are written and executed under one lock: a fork elsewhere would
/// inherit a script's write handle and make its exec fail with ETXTBSY.
static SPAWNS: Mutex<()> = Mutex::new(());

fn spawns() -> MutexGuard<'static, ()> {
    SPAWNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

const FAKE_ADAPTER: &str = r#"#!/bin/sh
here=$(dirname "$0")
read -r request
case "$request" in
  *'"action":"negotiate"'*)
    if [ -f "$here/bad-version" ]; then
      echo '{"protocol_version":99,"kind":"capabilities","capabilities":{"headless":true}}'
    elif [ -f "$here/garbage" ]; then
      echo 'not json'
    elif [ -f "$here/no-headless" ]; then
      echo '{"protocol_version":1,"kind":"capabilities","capabilities":{"headless":false}}'
    elif [ -f "$here/crash" ]; then
      exit 1
    else
      echo '{"protocol_version":1,"kind":"capabilities","capabilities":{"headless":true,"modes":["plan"],"ask_back":false}}'
    fi ;;
  *'"action":"prepare_launch"'*)
    if [ -f "$here/empty-program" ]; then
      echo '{"protocol_version":1,"kind":"launch","program":"","native_session":null}'
    elif [ -f "$here/wrong-kind" ]; then
      echo '{"protocol_version":1,"kind":"nope","program":"/bin/sh","native_session":null}'
    else
      args=$(cat "$here/launch-args" 2>/dev/null || echo '["-c","echo harness-ran"]')
      echo "{\"protocol_version\":1,\"kind\":\"launch\",\"program\":\"/bin/sh\",\"args\":$args,\"env\":{\"FAKE\":\"1\"},\"native_session\":\"fake-session\"}"
    fi ;;
  *'"action":"collect_handoff"'*)
    if [ -f "$here/collect-fail" ]; then exit 1; fi
    echo '{"protocol_version":1,"kind":"handoff","native_session":"fake-session","transcript":{"messages":[{"role":"user","text":"from the harness"},{"role":"assistant","text":"harness reply"}]},"usage":{"input_tokens":10,"output_tokens":5,"cached_input_tokens":2,"cost_usd":0.5,"model":"fake-model","turns":1,"plan_five_hour_percent":12.5,"plan_five_hour_resets_at":"2026-10-04T05:00:00Z","plan_week_percent":3.0,"plan_week_resets_at":"2026-10-09T00:00:00Z"}}' ;;
  *'"action":"start_thread"'*|*'"action":"send_message"'*|*'"action":"attach_thread"'*)
    if [ -f "$here/stream-garbage" ]; then echo 'not json'; exit 0; fi
    if [ -f "$here/stream-unknown" ]; then echo '{"event":"mystery"}'; exit 0; fi
    echo '{"event":"status","content":"working"}'
    case "$request" in
      *'"text":"ask'*)
        echo '{"event":"request","payload":{"id":"native-1","kind":"permission","title":"Bash ls"}}' ;;
    esac
    echo '{"event":"tool_call","content":"Bash","payload":{"name":"Bash"}}'
    echo '{"event":"tool_result","content":"ok"}'
    echo '{"event":"assistant_delta","content":"hello"}'
    echo '{"event":"assistant_message","role":"assistant","content":"hello from fake"}'
    echo '{"event":"usage","usage":{"input_tokens":3,"output_tokens":4,"context_used_tokens":100,"context_limit_tokens":1000,"model":"fake-model","plan_five_hour_percent":10.0,"plan_five_hour_resets_at":"2026-10-04T05:00:00Z"}}'
    echo '{"event":"native_session_update","native_session":"fake-session"}'
    if [ -f "$here/stream-fail" ]; then exit 2; fi
    echo '{"event":"turn_complete"}' ;;
  *'"action":"detach_thread"'*)
    if [ -f "$here/detach-fail" ]; then exit 1; fi
    echo '{}' ;;
esac
"#;

struct Workbench {
    root: PathBuf,
    state: PathBuf,
    adapters: PathBuf,
    home: PathBuf,
    workspace: PathBuf,
}

impl Workbench {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("mitos-commands-{}", unique()));
        let workbench = Self {
            state: root.join("state"),
            adapters: root.join("adapters"),
            home: root.join("home"),
            workspace: root.join("workspace"),
            root,
        };
        let _guard = spawns();
        for directory in [
            &workbench.state,
            &workbench.adapters,
            &workbench.home,
            &workbench.workspace,
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        Self::script(&workbench.adapters.join("mitos-fake"), FAKE_ADAPTER);
        workbench
    }

    fn script(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn flag(&self, name: &str) {
        fs::write(self.adapters.join(name), "").unwrap();
    }

    fn unflag(&self, name: &str) {
        let _ = fs::remove_file(self.adapters.join(name));
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mitos"));
        command
            .current_dir(&self.workspace)
            .env("MITOS_STATE_DIR", &self.state)
            .env("MITOS_ADAPTER_DIR", &self.adapters)
            .env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("HERMES_HOME")
            .env_remove("MITOS_THREAD_ID")
            .env_remove("MITOS_HOOK_DEBUG")
            .env_remove("MITOS_TUI");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        let _guard = spawns();
        self.command().args(args).output().unwrap()
    }

    fn run_with_stdin(&self, args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> Output {
        let _guard = spawns();
        let mut command = self.command();
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "mitos {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(!output.status.success(), "mitos {args:?} should fail");
        String::from_utf8(output.stderr).unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut full = args.to_vec();
        full.push("--json");
        serde_json::from_str(&self.ok(&full)).unwrap()
    }

    fn new_thread(&self) -> String {
        self.json(&["thread", "new", "--harness", "fake"])["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn events(&self, id: &str) -> Vec<Value> {
        self.json(&["thread", "sync", id])
            .as_array()
            .unwrap()
            .clone()
    }

    fn workspace_key(&self) -> String {
        let capture = self.root.join("tui-env");
        let tui = self.root.join("tui.sh");
        {
            let _guard = spawns();
            Self::script(
                &tui,
                &format!(
                    "#!/bin/sh\necho \"$MITOS_WORKSPACE_KEY\" > {}\n",
                    capture.display()
                ),
            );
        }
        let output = {
            let _guard = spawns();
            self.command().env("MITOS_TUI", &tui).output().unwrap()
        };
        assert!(output.status.success());
        fs::read_to_string(capture).unwrap().trim().to_owned()
    }
}

impl Drop for Workbench {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|event| event["kind"].as_str().unwrap())
        .collect()
}

#[test]
fn a_thread_runs_a_turn_and_lists_its_events() {
    let bench = Workbench::new();
    let created = bench.ok(&["thread", "new", "--harness", "fake"]);
    assert!(created.starts_with("Created thread "));
    let id = created
        .trim()
        .trim_start_matches("Created thread ")
        .to_owned();

    let listing = bench.ok(&["thread", "list"]);
    assert!(listing.contains(&id) && listing.contains("[fake]") && listing.contains("active"));
    assert_eq!(bench.json(&["thread", "list"]).as_array().unwrap().len(), 1);

    let sent = bench.ok(&["thread", "send", &id, "--message", "hello"]);
    assert!(sent.starts_with("Queued turn "));

    let events = bench.events(&id);
    let kinds = kinds(&events);
    for expected in [
        "user_message",
        "tool_call",
        "tool_result",
        "assistant_delta",
        "assistant_message",
        "usage",
    ] {
        assert!(kinds.contains(&expected), "missing {expected} in {kinds:?}");
    }
    let text = bench.ok(&["thread", "sync", &id, "--since", "0"]);
    assert!(text.contains("assistant_message: hello from fake"));
    assert_eq!(
        bench.json(&["thread", "sync", &id, "--since", "9999"]),
        Value::Array(Vec::new())
    );

    bench.ok(&["thread", "send", &id, "--message", "again"]);
    assert!(bench.events(&id).len() > events.len());

    let before_attach = bench.events(&id).len();
    bench.ok(&["thread", "attach", &id]);
    assert!(bench.events(&id).len() > before_attach);
}

#[test]
fn a_thread_without_a_harness_cannot_send() {
    let bench = Workbench::new();
    let id = bench.json(&["thread", "new"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        bench
            .err(&["thread", "send", &id, "--message", "hi"])
            .contains("no harness assigned")
    );
    assert!(bench.ok(&["thread", "attach", &id]).is_empty());
}

#[test]
fn notes_decisions_and_questions_become_events() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    assert!(
        bench
            .err(&["thread", "note", &id])
            .contains("provide --note, --decision, or --question")
    );
    bench.ok(&[
        "thread",
        "note",
        &id,
        "--note",
        "a note",
        "--decision",
        "a decision",
        "--question",
        "a question",
    ]);
    let events = bench.events(&id);
    let kinds = kinds(&events);
    assert!(kinds.contains(&"note") && kinds.contains(&"decision") && kinds.contains(&"question"));
}

#[test]
fn modes_switch_and_are_enforced_by_capabilities() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    assert!(
        bench
            .ok(&["thread", "mode", &id, "plan"])
            .contains("plan mode")
    );
    assert!(
        bench
            .ok(&["thread", "mode", &id, "plan"])
            .contains("plan mode")
    );
    bench.ok(&["thread", "send", &id, "--message", "plan it"]);
    bench.ok(&["thread", "mode", &id, "build"]);
    assert!(
        bench
            .events(&id)
            .iter()
            .any(|e| e["kind"] == "mode_changed")
    );
}

#[test]
fn a_harness_without_headless_mode_is_refused() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    bench.flag("no-headless");
    assert!(
        bench
            .err(&["thread", "send", &id, "--message", "hi"])
            .contains("no headless mode")
    );
    assert!(bench.ok(&["thread", "attach", &id]).is_empty());
}

#[test]
fn reassigning_and_compacting_rebind_the_thread() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    bench.ok(&["thread", "send", &id, "--message", "hello"]);

    assert!(
        bench
            .ok(&["thread", "reassign", &id, "--to", "fake"])
            .contains("Reassigned")
    );
    assert!(
        bench
            .ok(&["thread", "compact", &id])
            .contains("Compacted thread")
    );
    bench.ok(&["thread", "send", &id, "--message", "hello again"]);
    assert!(
        bench
            .ok(&["thread", "compact", &id, "--mode", "intelligent"])
            .contains("(intelligent)")
    );
    let events = bench.events(&id);
    let kinds = kinds(&events);
    assert!(kinds.contains(&"compaction"));
    assert!(kinds.contains(&"handoff_carryover"));
    assert!(kinds.contains(&"harness_unbound"));
}

#[test]
fn reassigning_fails_cleanly_when_collection_fails() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    bench.ok(&["thread", "send", &id, "--message", "hello"]);
    bench.flag("collect-fail");
    assert!(
        bench
            .err(&["thread", "reassign", &id, "--to", "fake"])
            .contains("adapter fake exited")
    );
}

#[test]
fn compacting_a_thread_without_a_harness_fails() {
    let bench = Workbench::new();
    let id = bench.json(&["thread", "new"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        bench
            .err(&["thread", "compact", &id])
            .contains("no harness assigned")
    );
}

#[test]
fn archiving_and_deleting_detach_the_harness() {
    let bench = Workbench::new();
    let archived = bench.new_thread();
    assert!(
        bench
            .ok(&["thread", "archive", &archived])
            .contains("Archived")
    );
    let deleted = bench.new_thread();
    assert!(
        bench
            .ok(&["thread", "delete", &deleted])
            .contains("Deleted")
    );
    let remaining = bench.json(&["thread", "list"]);
    assert_eq!(remaining.as_array().unwrap().len(), 1);

    let failing = bench.new_thread();
    bench.flag("detach-fail");
    assert!(
        bench
            .err(&["thread", "archive", &failing])
            .contains("exited")
    );
    assert!(
        bench
            .err(&["thread", "delete", &failing])
            .contains("exited")
    );
}

#[test]
fn requests_are_listed_and_answers_fail_once_the_turn_is_gone() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    assert!(bench.ok(&["thread", "requests", &id]).is_empty());

    bench.ok(&["thread", "send", &id, "--message", "hello"]);
    bench.ok(&["thread", "send", &id, "--message", "ask permission"]);
    let pending = bench.json(&["thread", "requests", &id]);
    let request = pending[0]["id"].as_str().unwrap().to_owned();
    assert!(
        bench
            .ok(&["thread", "requests", &id])
            .contains("[permission]")
    );

    assert!(
        !bench
            .err(&[
                "thread",
                "answer",
                &id,
                "--request",
                "missing",
                "--response",
                "{}"
            ])
            .is_empty()
    );
    assert!(
        bench
            .err(&[
                "thread",
                "answer",
                &id,
                "--request",
                &request,
                "--response",
                "{\"allow\":true}",
            ])
            .contains("no longer running")
    );
    assert!(
        bench
            .json(&["thread", "requests", &id])
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        bench
            .err(&[
                "thread",
                "answer",
                &id,
                "--request",
                &request,
                "--response",
                "yes"
            ])
            .contains("not pending")
    );
    let other = bench.new_thread();
    assert!(
        bench
            .err(&[
                "thread",
                "answer",
                &other,
                "--request",
                &request,
                "--response",
                "yes"
            ])
            .contains("does not belong")
    );
}

#[test]
fn adapter_failures_surface_as_errors() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    for (flag, expected) in [
        ("bad-version", "protocol version 99"),
        ("garbage", "valid negotiate JSON"),
        ("crash", "exited with"),
    ] {
        bench.flag(flag);
        let message = bench.err(&["thread", "send", &id, "--message", "hi"]);
        assert!(message.contains(expected), "{flag}: {message}");
        bench.unflag(flag);
    }
    for (flag, expected) in [
        ("stream-garbage", "invalid event JSON"),
        ("stream-unknown", "unknown event kind"),
        ("stream-fail", "exited with"),
    ] {
        bench.flag(flag);
        let message = bench.err(&["thread", "send", &id, "--message", "hi"]);
        assert!(message.contains(expected), "{flag}: {message}");
        bench.unflag(flag);
    }
}

#[test]
fn an_adapter_that_does_not_exist_is_reported() {
    let bench = Workbench::new();
    let id = bench.json(&["thread", "new", "--harness", "missing"])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        bench
            .err(&["thread", "send", &id, "--message", "hi"])
            .contains("could not start adapter missing")
    );
}

#[test]
fn views_report_workspace_state_as_json() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    let key = bench.workspace_key();

    assert_eq!(bench.ok(&["view", "empty", &id]).trim(), "true");
    bench.ok(&["thread", "send", &id, "--message", "remember this"]);
    assert_eq!(bench.ok(&["view", "empty", &id]).trim(), "false");

    let threads = bench.ok(&["view", "threads", "--workspace-key", &key]);
    assert!(threads.contains("remember this"));
    let history = bench.ok(&["view", "history", "--workspace-key", &key, "--limit", "5"]);
    assert!(history.contains("remember this"));
    let usage = bench.ok(&["view", "usage", &id, "--harness", "fake"]);
    assert!(usage.contains("fake-model"));
    assert_eq!(
        bench
            .ok(&["view", "usage", &id, "--harness", "other"])
            .trim(),
        "null"
    );
}

#[test]
fn threads_in_a_git_checkout_resolve_to_the_repository_root() {
    let bench = Workbench::new();
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&bench.workspace)
        .status()
        .unwrap();
    assert!(status.success());
    fs::create_dir_all(bench.workspace.join("nested")).unwrap();
    let nested = bench.workspace.join("nested");
    let nested = nested.to_str().unwrap();
    let id = bench.json(&["thread", "new", "--workspace", nested])["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let listed = bench.json(&[
        "thread",
        "list",
        "--workspace",
        bench.workspace.to_str().unwrap(),
    ]);
    assert_eq!(listed[0]["id"], id.as_str());
}

#[test]
fn an_unknown_workspace_has_no_threads_and_a_missing_one_errors() {
    let bench = Workbench::new();
    assert_eq!(bench.json(&["thread", "list"]), Value::Array(Vec::new()));
    assert!(
        bench
            .err(&["thread", "list", "--workspace", "/nonexistent/place"])
            .contains("cannot access")
    );
}

#[test]
fn hooks_install_report_and_record_observations() {
    let bench = Workbench::new();
    let before = bench.ok(&["hooks", "status"]);
    assert!(before.contains("not installed"));
    assert!(before.contains("no data received yet"));
    assert!(bench.json(&["hooks", "status"]).as_array().unwrap().len() >= 4);

    let installed = bench.ok(&["hooks", "init"]);
    assert!(installed.contains("installed"));
    assert!(installed.contains("mitos hooks status"));
    let again = bench.json(&["hooks", "init"]);
    assert!(
        again
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["result"] == "already_installed" || entry["result"] == "manual" })
    );
    assert!(
        bench
            .ok(&["hooks", "init", "--harness", "claude"])
            .contains("claude")
    );
    assert!(
        bench
            .ok(&["hooks", "init", "--harness", "bogus"])
            .contains("bogus")
    );

    let payload = r#"{"session_id":"s1","cwd":"/ws","model":{"id":"claude-x"},"cost":{"total_cost_usd":1.5},"context_window":{"context_window_size":1000,"used_percentage":10}}"#;
    let passthrough = bench.run_with_stdin(&["hook", "claude", "--passthrough"], payload, &[]);
    assert!(passthrough.status.success());
    assert_eq!(String::from_utf8_lossy(&passthrough.stdout), payload);

    let id = bench.new_thread();
    let tagged = bench.run_with_stdin(
        &["hook", "codex", "--event", "SessionStart"],
        r#"{"session_id":"c1"}"#,
        &[("MITOS_THREAD_ID", id.as_str())],
    );
    assert!(tagged.status.success());

    let after = bench.ok(&["hooks", "status"]);
    assert!(after.contains("last data"), "{after}");
}

#[test]
fn hooks_never_fail_the_harness() {
    let bench = Workbench::new();
    for (args, stdin) in [
        (vec!["hook", "claude"], "not json"),
        (vec!["hook", "claude"], "[1,2]"),
        (vec!["hook", "bogus"], "{}"),
    ] {
        let output = bench.run_with_stdin(&args, stdin, &[]);
        assert!(output.status.success());
    }
    let debug = bench.run_with_stdin(
        &["hook", "claude"],
        "not json",
        &[("MITOS_HOOK_DEBUG", "1")],
    );
    assert!(debug.status.success());
    assert!(String::from_utf8_lossy(&debug.stderr).contains("mitos hook claude"));
}

#[test]
fn launching_the_tui_passes_the_workspace_and_reports_failures() {
    let bench = Workbench::new();
    let capture = bench.root.join("tui-env");
    let tui = bench.root.join("tui.sh");
    {
        let _guard = spawns();
        Workbench::script(
            &tui,
            &format!(
                "#!/bin/sh\necho \"$MITOS_WORKSPACE_ROOT|$MITOS_STATE_DIR|$MITOS_CORE\" > {}\n",
                capture.display()
            ),
        );
    }
    let output = {
        let _guard = spawns();
        bench.command().env("MITOS_TUI", &tui).output().unwrap()
    };
    assert!(output.status.success());
    let seen = fs::read_to_string(&capture).unwrap();
    assert!(seen.contains("workspace"), "{seen}");
    assert!(seen.contains("state"));

    {
        let _guard = spawns();
        Workbench::script(&tui, "#!/bin/sh\nexit 3\n");
    }
    let failed = {
        let _guard = spawns();
        bench.command().env("MITOS_TUI", &tui).output().unwrap()
    };
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("Mitos TUI exited"));

    let missing = {
        let _guard = spawns();
        bench
            .command()
            .env("MITOS_TUI", bench.root.join("absent.sh"))
            .output()
            .unwrap()
    };
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not start Mitos TUI"));
}

#[test]
fn a_ts_tui_entry_runs_through_bun() {
    let bench = Workbench::new();
    let entry = bench.root.join("entry.ts");
    fs::write(&entry, "").unwrap();
    let output = {
        let _guard = spawns();
        bench
            .command()
            .env("MITOS_TUI", &entry)
            .env("MITOS_BUN", bench.root.join("no-bun"))
            .output()
            .unwrap()
    };
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not start Mitos TUI"));
}

#[test]
fn the_packaged_tui_is_looked_up_under_the_library_dir() {
    let bench = Workbench::new();
    let library = bench.root.join("lib");
    fs::create_dir_all(library.join("tui")).unwrap();
    fs::write(library.join("tui/main.mjs"), "").unwrap();
    let output = {
        let _guard = spawns();
        bench
            .command()
            .env("MITOS_LIBRARY_DIR", &library)
            .env("MITOS_BUN", bench.root.join("no-bun"))
            .output()
            .unwrap()
    };
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not start Mitos TUI"));

    let empty = {
        let _guard = spawns();
        bench
            .command()
            .env("MITOS_LIBRARY_DIR", bench.root.join("nothing"))
            .env("PATH", "")
            .output()
            .unwrap()
    };
    assert!(String::from_utf8_lossy(&empty.stderr).contains("could not start Mitos TUI"));
}

/// Built-in adapters, with no harness program anywhere on `PATH`.
fn native(bench: &Workbench) -> Command {
    let mut command = bench.command();
    command
        .env_remove("MITOS_ADAPTER_DIR")
        .env("PATH", bench.root.join("empty-path"));
    command
}

fn native_run(bench: &Workbench, args: &[&str]) -> Output {
    let _guard = spawns();
    native(bench).args(args).output().unwrap()
}

fn native_json(bench: &Workbench, args: &[&str]) -> Value {
    let mut full = args.to_vec();
    full.push("--json");
    let output = native_run(bench, &full);
    assert!(
        output.status.success(),
        "mitos {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn built_in_adapters_start_resume_attach_and_detach_their_harness() {
    for harness in ["claude", "codex", "hermes", "opencode"] {
        let bench = Workbench::new();
        fs::create_dir_all(bench.root.join("empty-path")).unwrap();
        let id = native_json(&bench, &["thread", "new", "--harness", harness])["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let missing = format!("could not start {harness}");
        let first = native_run(&bench, &["thread", "send", &id, "--message", "hello"]);
        assert!(!first.status.success(), "{harness}");
        assert!(
            stderr_of(&first).contains(&missing),
            "{harness}: {}",
            stderr_of(&first)
        );

        // Without a terminal the handoff prints, then raw mode cannot start.
        let entered = native_run(
            &bench,
            &["enter", harness, "--thread", &id, "--native-session", "s-1"],
        );
        assert!(
            String::from_utf8_lossy(&entered.stdout)
                .contains(&format!("Mitos handoff to {harness}"))
        );
        assert!(
            stderr_of(&entered).contains("terminal session"),
            "{}",
            stderr_of(&entered)
        );

        let resumed = native_run(&bench, &["thread", "send", &id, "--message", "again"]);
        assert!(
            stderr_of(&resumed).contains(&missing),
            "{harness}: {}",
            stderr_of(&resumed)
        );

        assert!(
            native_run(&bench, &["thread", "attach", &id])
                .status
                .success()
        );
        assert!(
            native_run(&bench, &["thread", "reassign", &id, "--to", harness])
                .status
                .success()
        );
        assert!(
            native_run(&bench, &["thread", "archive", &id])
                .status
                .success()
        );
    }
}

fn enter_in_pty(bench: &Workbench, args: &[&str]) -> (String, bool) {
    let _guard = spawns();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_mitos"));
    command.args(args);
    command.cwd(&bench.workspace);
    command.env("MITOS_STATE_DIR", &bench.state);
    command.env("HOME", &bench.home);
    command.env("MITOS_ADAPTER_DIR", &bench.adapters);
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let _writer = pair.master.take_writer().unwrap();
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    let status = child.wait().unwrap();
    (
        String::from_utf8_lossy(&output).into_owned(),
        status.success(),
    )
}

#[test]
fn entering_a_native_harness_hands_off_runs_and_records_notes() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    bench.ok(&["thread", "send", &id, "--message", "earlier"]);

    let (output, success) = enter_in_pty(
        &bench,
        &[
            "enter",
            "fake",
            "--thread",
            &id,
            "--note",
            "back from the harness",
            "--native-session",
            "sess-2",
        ],
    );
    assert!(success, "{output}");
    assert!(output.contains("Mitos handoff to fake"), "{output}");
    assert!(output.contains("harness-ran"), "{output}");
    assert!(output.contains("fake exited."), "{output}");
    assert!(
        bench
            .events(&id)
            .iter()
            .any(|e| e["content"] == "back from the harness")
    );

    fs::write(bench.adapters.join("launch-args"), r#"["-c","exit 3"]"#).unwrap();
    let (output, success) = enter_in_pty(&bench, &["enter", "fake", "--thread", &id]);
    assert!(success, "{output}");
    assert!(output.contains("fake exited with"), "{output}");

    bench.flag("collect-fail");
    let (output, _) = enter_in_pty(&bench, &["enter", "fake", "--thread", &id]);
    assert!(
        output.contains("could not mine the fake handoff"),
        "{output}"
    );
}

#[test]
fn entering_with_a_bad_launch_plan_fails() {
    let bench = Workbench::new();
    let id = bench.new_thread();
    for flag in ["empty-program", "wrong-kind"] {
        bench.flag(flag);
        let (output, success) = enter_in_pty(&bench, &["enter", "fake", "--thread", &id]);
        assert!(!success, "{flag}: {output}");
        bench.unflag(flag);
    }
}
