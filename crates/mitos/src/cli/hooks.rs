use std::io::{self, Read, Write};
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::args::{HookArgs, HooksCommand};
use super::outputs::emit;
use crate::hooks::{self, HookStatus, InitOutcome, InitResult, Locations, Trust};
use crate::store::Store;

/// Hook payloads are small; a runaway pipe must not be buffered whole.
const MAX_PAYLOAD_BYTES: u64 = 1 << 20;

/// Runs inside a harness hook, so it must never fail or slow the harness:
/// it forwards stdin first when asked, then records on a best-effort basis.
#[allow(clippy::unnecessary_wraps)]
pub fn collect(state_dir: Option<PathBuf>, args: &HookArgs) -> Result<()> {
    let mut input = Vec::new();
    let _ = io::stdin().take(MAX_PAYLOAD_BYTES).read_to_end(&mut input);
    if args.passthrough {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&input);
        let _ = stdout.flush();
    }
    if let Err(error) = record(state_dir, args, &input)
        && std::env::var_os("MITOS_HOOK_DEBUG").is_some()
    {
        eprintln!("mitos hook {}: {error:#}", args.harness);
    }
    Ok(())
}

fn record(state_dir: Option<PathBuf>, args: &HookArgs, input: &[u8]) -> Result<()> {
    let payload: Value = serde_json::from_slice(input)?;
    let Some(observation) = hooks::parse(&args.harness, args.event.as_deref(), &payload) else {
        return Ok(());
    };
    let store = Store::new(state_dir)?;
    let thread = std::env::var("MITOS_THREAD_ID")
        .ok()
        .filter(|id| !id.is_empty());
    store.record_observation(&observation, thread.as_deref())
}

pub fn dispatch(store: &Store, command: HooksCommand) -> Result<()> {
    let locations = Locations::from_env();
    match command {
        HooksCommand::Status { json } => {
            let seen = store.hooks_last_seen()?;
            let mut report = hooks::status(&locations);
            for status in &mut report {
                status.last_seen = seen
                    .iter()
                    .find(|entry| entry.harness == status.harness)
                    .map(|entry| entry.observed_at.clone());
            }
            emit(json, report, |report| {
                print_lines(status_lines(&report, Utc::now()));
            })
        }
        HooksCommand::Init { harnesses, json } => {
            let outcomes = hooks::init(&locations, &harnesses);
            emit(json, outcomes, |outcomes| {
                print_lines(init_lines(&outcomes));
            })
        }
    }
}

fn approval_step(harness: &str) -> Option<&'static str> {
    match harness {
        "codex" => Some("approve it with /hooks inside Codex"),
        "hermes" => Some("approve it on Hermes's first run, or set hooks_auto_accept: true"),
        _ => None,
    }
}

fn relative_time(iso: &str, now: DateTime<Utc>) -> String {
    let Ok(seen) = DateTime::parse_from_rfc3339(iso) else {
        return "at an unknown time".into();
    };
    let seconds = (now - seen.with_timezone(&Utc)).num_seconds();
    match seconds {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86_400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

fn state_of(status: &HookStatus) -> String {
    match (status.installed, status.trust) {
        (false, _) => "not installed".into(),
        (true, Trust::Untrusted) => match approval_step(&status.harness) {
            Some(step) => format!("installed, NOT APPROVED yet: {step}"),
            None => "installed, NOT APPROVED yet by the harness".into(),
        },
        (true, _) => "installed".into(),
    }
}

fn status_lines(statuses: &[HookStatus], now: DateTime<Utc>) -> Vec<String> {
    let mut lines = Vec::new();
    for status in statuses {
        lines.push(format!("{:<9} {}", status.harness, state_of(status)));
        lines.push(format!("          {}", status.target));
        if let Some(reason) = &status.blocked_by {
            lines.push(format!("          cannot write: {reason}"));
        }
        for problem in &status.problems {
            lines.push(format!("          problem: {problem}"));
        }
        lines.push(match &status.last_seen {
            Some(seen) => format!("          last data {}", relative_time(seen, now)),
            None => "          no data received yet".into(),
        });
    }
    lines.push("Run `mitos hooks init` to install; it backs each file up first.".into());
    lines
}

fn init_lines(outcomes: &[InitOutcome]) -> Vec<String> {
    let mut lines = Vec::new();
    for outcome in outcomes {
        let label = match outcome.result {
            InitResult::Installed => "installed",
            InitResult::AlreadyInstalled => "already installed",
            InitResult::Skipped => "skipped",
            InitResult::Manual => "manual step",
        };
        lines.push(format!(
            "{:<9} {label}: {}",
            outcome.harness, outcome.message
        ));
        if let Some(backup) = &outcome.backup {
            lines.push(format!("          backup: {backup}"));
        }
        for line in outcome.snippet.iter().flat_map(|snippet| snippet.lines()) {
            lines.push(format!("          | {line}"));
        }
    }
    if outcomes.iter().any(|o| o.result == InitResult::Installed) {
        lines.push(
            "Run `mitos hooks status` to check that each harness has approved its hook.".into(),
        );
    }
    lines
}

fn print_lines(lines: Vec<String>) {
    for line in lines {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(harness: &str) -> HookStatus {
        HookStatus {
            harness: harness.into(),
            target: format!("/cfg/{harness}.json"),
            installed: true,
            trust: Trust::Trusted,
            blocked_by: None,
            problems: Vec::new(),
            last_seen: None,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-03T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn relative_time_buckets_match_the_tui() {
        let at = |iso| relative_time(iso, now());
        assert_eq!(at("2026-10-03T11:59:30Z"), "just now");
        assert_eq!(at("2026-10-03T11:55:00Z"), "5m ago");
        assert_eq!(at("2026-10-03T09:00:00Z"), "3h ago");
        assert_eq!(at("2026-10-01T12:00:00Z"), "2d ago");
        assert_eq!(at("garbage"), "at an unknown time");
    }

    #[test]
    fn untrusted_hooks_name_the_harness_specific_approval_step() {
        let mut codex = status("codex");
        codex.trust = Trust::Untrusted;
        let mut claude = status("claude");
        claude.trust = Trust::Untrusted;
        let lines = status_lines(&[codex, claude], now());
        assert_eq!(
            lines[0],
            "codex     installed, NOT APPROVED yet: approve it with /hooks inside Codex"
        );
        assert_eq!(
            lines[3],
            "claude    installed, NOT APPROVED yet by the harness"
        );
    }

    #[test]
    fn status_reports_problems_last_data_and_a_footer() {
        let mut broken = status("claude");
        broken.installed = false;
        broken.blocked_by = Some("read-only".into());
        broken.problems = vec!["invalid JSON".into()];
        broken.last_seen = Some("2026-10-03T11:55:00Z".into());
        let lines = status_lines(&[broken, status("opencode")], now());
        assert_eq!(
            lines[..5],
            [
                "claude    not installed",
                "          /cfg/claude.json",
                "          cannot write: read-only",
                "          problem: invalid JSON",
                "          last data 5m ago",
            ]
        );
        assert_eq!(lines[7], "          no data received yet");
        assert_eq!(
            lines.last().map(String::as_str),
            Some("Run `mitos hooks init` to install; it backs each file up first.")
        );
    }

    #[test]
    fn init_points_at_status_only_after_something_was_installed() {
        let skipped = InitOutcome::new("bogus", InitResult::Skipped, "unknown harness");
        assert_eq!(init_lines(std::slice::from_ref(&skipped)).len(), 1);

        let installed = InitOutcome::new("codex", InitResult::Installed, "ok").with_snippet("a\nb");
        let lines = init_lines(&[installed]);
        assert_eq!(
            lines[..3],
            ["codex     installed: ok", "          | a", "          | b"]
        );
        assert!(lines[3].starts_with("Run `mitos hooks status`"));
    }
}
