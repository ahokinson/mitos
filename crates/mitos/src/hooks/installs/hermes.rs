use std::fmt::Write;
use std::path::Path;

use serde_json::Value;

use crate::hooks::files::{backup, blocked_reason, read_text, write_atomic};
use crate::hooks::reports::{HookStatus, InitOutcome, InitResult, Trust};

const HARNESS: &str = "hermes";
const MARKER: &str = "mitos hook hermes";
const EVENTS: [&str; 2] = ["on_session_start", "on_session_end"];

pub fn status(config: &Path, allowlist: &Path) -> HookStatus {
    let text = read_text(config);
    let installed = text.as_deref().is_some_and(|text| text.contains(MARKER));
    let trust = if !installed {
        Trust::NotApplicable
    } else if text.as_deref().is_some_and(auto_accepts)
        || EVENTS.iter().all(|e| approved(allowlist, e))
    {
        Trust::Trusted
    } else {
        Trust::Untrusted
    };
    HookStatus {
        harness: HARNESS.into(),
        target: config.display().to_string(),
        installed,
        trust,
        blocked_by: blocked_reason(config),
        problems: Vec::new(),
        last_seen: None,
    }
}

pub fn init(config: &Path) -> InitOutcome {
    if let Some(reason) = blocked_reason(config) {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {reason}"),
        )
        .with_snippet(block());
    }
    let Some(text) = read_text(config) else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: config.yaml not found; run hermes once so it exists",
        )
        .with_snippet(block());
    };
    if text.contains(MARKER) {
        return InitOutcome::new(
            HARNESS,
            InitResult::AlreadyInstalled,
            "config.yaml already has the hooks",
        );
    }
    if has_hooks_key(&text) {
        return InitOutcome::new(
            HARNESS,
            InitResult::Manual,
            "config.yaml already has a hooks: block, which Mitos will not edit; add these entries to it",
        )
        .with_snippet(entries());
    }
    let mut updated = text;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&block());
    match backup(config).and_then(|saved| write_atomic(config, &updated).map(|()| saved)) {
        Ok(saved) => InitOutcome::new(
            HARNESS,
            InitResult::Installed,
            "added the hooks; Hermes asks you to approve each one on first use, or set hooks_auto_accept: true",
        )
        .with_backup(saved),
        Err(error) => InitOutcome::new(HARNESS, InitResult::Skipped, format!("not changed: {error}")),
    }
}

fn entries() -> String {
    EVENTS.iter().fold(String::new(), |mut text, event| {
        let _ = writeln!(text, "  {event}:\n    - command: \"{MARKER}\"");
        text
    })
}

fn block() -> String {
    format!("hooks:\n{}", entries())
}

/// A top-level `hooks:` key, including the inline `hooks: {}` form.
fn has_hooks_key(text: &str) -> bool {
    text.lines().any(|line| line.starts_with("hooks:"))
}

fn auto_accepts(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim() == "hooks_auto_accept: true")
}

fn approved(allowlist: &Path, event: &str) -> bool {
    read_text(allowlist)
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("approvals")?.as_array().cloned())
        .is_some_and(|approvals| {
            approvals.iter().any(|approval| {
                approval.get("event").and_then(Value::as_str) == Some(event)
                    && approval
                        .get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.contains(MARKER))
            })
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::hooks::files::scratch_dir;
    use crate::hooks::reports::{InitResult, Trust};

    use super::{init, status};

    #[test]
    fn a_config_without_hooks_gets_the_block_appended_and_a_backup() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        fs::write(&config, "model: x\nhooks_auto_accept: false").unwrap();

        let outcome = init(&config);

        assert_eq!(outcome.result, InitResult::Installed);
        assert!(outcome.backup.is_some());
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.starts_with("model: x\nhooks_auto_accept: false\nhooks:\n"));
        assert!(text.contains("on_session_start:\n    - command: \"mitos hook hermes\""));
        assert!(text.contains("on_session_end:"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_existing_hooks_block_is_never_edited() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        let original = "hooks:\n  on_session_start:\n    - command: other\n";
        fs::write(&config, original).unwrap();

        let outcome = init(&config);

        assert_eq!(outcome.result, InitResult::Manual);
        assert!(outcome.snippet.unwrap().contains("mitos hook hermes"));
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_config_is_not_created() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");

        assert_eq!(init(&config).result, InitResult::Skipped);
        assert!(!config.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_twice_changes_nothing_the_second_time() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        fs::write(&config, "model: x\n").unwrap();
        init(&config);
        let after_first = fs::read_to_string(&config).unwrap();

        assert_eq!(init(&config).result, InitResult::AlreadyInstalled);
        assert_eq!(fs::read_to_string(&config).unwrap(), after_first);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hooks_are_untrusted_until_every_event_is_in_the_allowlist() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        let allowlist = dir.join("allow.json");
        fs::write(&config, "model: x\n").unwrap();
        init(&config);
        assert_eq!(status(&config, &allowlist).trust, Trust::Untrusted);

        fs::write(
            &allowlist,
            r#"{"approvals":[{"event":"on_session_start","command":"mitos hook hermes"}]}"#,
        )
        .unwrap();
        assert_eq!(status(&config, &allowlist).trust, Trust::Untrusted);

        fs::write(
            &allowlist,
            r#"{"approvals":[
              {"event":"on_session_start","command":"mitos hook hermes"},
              {"event":"on_session_end","command":"mitos hook hermes"}]}"#,
        )
        .unwrap();
        assert_eq!(status(&config, &allowlist).trust, Trust::Trusted);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auto_accept_counts_as_trusted() {
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        fs::write(&config, "hooks_auto_accept: true\n").unwrap();
        init(&config);

        assert_eq!(
            status(&config, &dir.join("none.json")).trust,
            Trust::Trusted
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_read_only_config_is_reported_with_the_snippet() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir();
        let config = dir.join("config.yaml");
        fs::write(&config, "model: x\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o444)).unwrap();

        assert!(status(&config, &dir.join("none.json")).blocked_by.is_some());
        let outcome = init(&config);

        assert_eq!(outcome.result, InitResult::Skipped);
        assert!(outcome.snippet.unwrap().starts_with("hooks:\n"));
        assert_eq!(fs::read_to_string(&config).unwrap(), "model: x\n");
        fs::remove_dir_all(dir).unwrap();
    }
}
