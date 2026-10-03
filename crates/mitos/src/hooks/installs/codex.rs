use std::path::Path;

use serde_json::{Map, Value, json};

use crate::hooks::files::{backup, blocked_reason, read_text, write_atomic};
use crate::hooks::reports::{HookStatus, InitOutcome, InitResult, Trust};

const HARNESS: &str = "codex";
const MARKER: &str = "mitos hook codex";
const EVENTS: [&str; 3] = ["SessionStart", "Stop", "SessionEnd"];
const TIMEOUT_SECONDS: u64 = 10;

pub fn status(hooks: &Path, config: &Path) -> HookStatus {
    let mut problems = Vec::new();
    let document = match read_hooks(hooks) {
        Ok(document) => document,
        Err(problem) => {
            problems.push(problem);
            None
        }
    };
    let positions: Vec<Option<(usize, usize)>> = EVENTS
        .iter()
        .map(|event| document.as_ref().and_then(|doc| position_of(doc, event)))
        .collect();
    let installed = positions.iter().all(Option::is_some);
    let trust = if installed {
        let trusted = EVENTS.iter().zip(&positions).all(|(event, position)| {
            position.is_some_and(|(group, handler)| {
                is_trusted(config, &trust_key(hooks, event, group, handler))
            })
        });
        if trusted {
            Trust::Trusted
        } else {
            Trust::Untrusted
        }
    } else {
        Trust::NotApplicable
    };
    HookStatus {
        harness: HARNESS.into(),
        target: hooks.display().to_string(),
        installed,
        trust,
        blocked_by: blocked_reason(hooks),
        problems,
        last_seen: None,
    }
}

pub fn init(hooks: &Path) -> InitOutcome {
    if let Some(reason) = blocked_reason(hooks) {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {reason}"),
        )
        .with_snippet(snippet());
    }
    let mut document = match read_hooks(hooks) {
        Ok(document) => document.unwrap_or_else(|| json!({})),
        Err(problem) => {
            return InitOutcome::new(
                HARNESS,
                InitResult::Skipped,
                format!("not changed: {problem}; fix it, then run init again"),
            );
        }
    };
    let Some(root) = document.as_object_mut() else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: hooks.json is not a JSON object",
        );
    };
    let events = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(events) = events.as_object_mut() else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: \"hooks\" is not an object",
        );
    };
    let mut added = 0;
    for event in EVENTS {
        let groups = events
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(groups) = groups.as_array_mut() else {
            return InitOutcome::new(
                HARNESS,
                InitResult::Skipped,
                format!("not changed: hooks.{event} is not an array"),
            );
        };
        if !groups.iter().any(group_has_marker) {
            groups.push(entry());
            added += 1;
        }
    }
    if added == 0 {
        return InitOutcome::new(
            HARNESS,
            InitResult::AlreadyInstalled,
            "hooks.json already has the hooks",
        );
    }
    let Ok(mut text) = serde_json::to_string_pretty(&document) else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: could not serialize hooks.json",
        );
    };
    text.push('\n');
    match backup(hooks).and_then(|saved| write_atomic(hooks, &text).map(|()| saved)) {
        Ok(saved) => InitOutcome::new(
            HARNESS,
            InitResult::Installed,
            "added the hooks; Codex skips them until you approve them with /hooks inside Codex",
        )
        .with_backup(saved),
        Err(error) => InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {error}"),
        ),
    }
}

fn entry() -> Value {
    json!({
        "hooks": [{ "type": "command", "command": MARKER, "timeout": TIMEOUT_SECONDS }]
    })
}

fn snippet() -> String {
    let events: Map<String, Value> = EVENTS
        .iter()
        .map(|event| ((*event).to_owned(), Value::Array(vec![entry()])))
        .collect();
    serde_json::to_string_pretty(&json!({ "hooks": events })).unwrap_or_default()
}

fn group_has_marker(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|handlers| handlers.iter().any(handler_has_marker))
}

fn handler_has_marker(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(MARKER))
}

/// (group index, handler index) of the first of our handlers for `event`.
fn position_of(document: &Value, event: &str) -> Option<(usize, usize)> {
    let groups = document.get("hooks")?.get(event)?.as_array()?;
    groups.iter().enumerate().find_map(|(group, value)| {
        let handlers = value.get("hooks")?.as_array()?;
        let handler = handlers.iter().position(handler_has_marker)?;
        Some((group, handler))
    })
}

/// How Codex keys a hook in `[hooks.state]`: path, `snake_case` event, indices.
fn trust_key(hooks: &Path, event: &str, group: usize, handler: usize) -> String {
    let mut snake = String::new();
    for (index, character) in event.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.extend(character.to_lowercase());
    }
    format!("{}:{snake}:{group}:{handler}", hooks.display())
}

/// Whether Codex has recorded trust for the hook. The recorded hash is not
/// verified, so an edited hook at the same position still reads as trusted.
fn is_trusted(config: &Path, key: &str) -> bool {
    read_text(config)
        .and_then(|text| text.parse::<toml::Value>().ok())
        .and_then(|value| {
            value
                .get("hooks")?
                .get("state")?
                .get(key)?
                .get("trusted_hash")?
                .as_str()
                .map(str::to_owned)
        })
        .is_some()
}

fn read_hooks(path: &Path) -> Result<Option<Value>, String> {
    let Some(text) = read_text(path) else {
        return Ok(None);
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("{} is not valid JSON ({error})", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::fs;

    use crate::hooks::files::scratch_dir;
    use crate::hooks::reports::{InitResult, Trust};

    use super::{init, status, trust_key};

    #[test]
    fn init_adds_every_event_and_leaves_them_untrusted() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        let config = dir.join("config.toml");
        assert!(!status(&hooks, &config).installed);

        let outcome = init(&hooks);

        assert_eq!(outcome.result, InitResult::Installed);
        let report = status(&hooks, &config);
        assert!(report.installed);
        assert_eq!(report.trust, Trust::Untrusted);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recorded_trust_for_every_hook_reads_as_trusted() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        let config = dir.join("config.toml");
        init(&hooks);
        let mut toml = String::from("[hooks.state]\n");
        for event in ["SessionStart", "Stop", "SessionEnd"] {
            writeln!(
                toml,
                "[hooks.state.\"{}\"]\ntrusted_hash = \"sha256:abc\"",
                trust_key(&hooks, event, 0, 0)
            )
            .unwrap();
        }
        fs::write(&config, toml).unwrap();

        assert_eq!(status(&hooks, &config).trust, Trust::Trusted);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn one_unapproved_hook_makes_the_whole_set_untrusted() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        let config = dir.join("config.toml");
        init(&hooks);
        fs::write(
            &config,
            format!(
                "[hooks.state.\"{}\"]\ntrusted_hash = \"sha256:abc\"\n",
                trust_key(&hooks, "SessionStart", 0, 0)
            ),
        )
        .unwrap();

        assert_eq!(status(&hooks, &config).trust, Trust::Untrusted);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trust_keys_use_snake_case_events_and_the_hooks_path() {
        let key = trust_key(
            std::path::Path::new("/h/.codex/hooks.json"),
            "SessionStart",
            1,
            2,
        );

        assert_eq!(key, "/h/.codex/hooks.json:session_start:1:2");
    }

    #[test]
    fn existing_hooks_are_kept_and_ours_land_after_them() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        fs::write(
            &hooks,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"other"}]}]}}"#,
        )
        .unwrap();

        init(&hooks);

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&hooks).unwrap()).unwrap();
        let groups = written["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["hooks"][0]["command"], "other");
        assert_eq!(groups[1]["hooks"][0]["command"], "mitos hook codex");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_twice_changes_nothing_the_second_time() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        init(&hooks);
        let after_first = fs::read_to_string(&hooks).unwrap();

        assert_eq!(init(&hooks).result, InitResult::AlreadyInstalled);
        assert_eq!(fs::read_to_string(&hooks).unwrap(), after_first);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_is_never_overwritten() {
        let dir = scratch_dir();
        let hooks = dir.join("hooks.json");
        fs::write(&hooks, "nope").unwrap();

        assert_eq!(init(&hooks).result, InitResult::Skipped);
        assert_eq!(fs::read_to_string(&hooks).unwrap(), "nope");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_read_only_directory_is_reported_with_a_snippet() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir();
        let locked = dir.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let hooks = locked.join("hooks.json");

        assert!(
            status(&hooks, &dir.join("config.toml"))
                .blocked_by
                .is_some()
        );
        let outcome = init(&hooks);

        assert_eq!(outcome.result, InitResult::Skipped);
        assert!(outcome.snippet.unwrap().contains("mitos hook codex"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
