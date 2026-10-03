use std::path::Path;

use serde_json::{Value, json};

use crate::hooks::files::{backup, blocked_reason, read_text, write_atomic};
use crate::hooks::reports::{HookStatus, InitOutcome, InitResult, Trust};

const HARNESS: &str = "claude";
const MARKER: &str = "mitos hook claude";

pub fn status(settings: &Path) -> HookStatus {
    let mut problems = Vec::new();
    let installed = match read_settings(settings) {
        Ok(Some(document)) => statusline_command(&document).is_some_and(|c| c.contains(MARKER)),
        Ok(None) => false,
        Err(problem) => {
            problems.push(problem);
            false
        }
    };
    HookStatus {
        harness: HARNESS.into(),
        target: settings.display().to_string(),
        installed,
        trust: Trust::NotApplicable,
        blocked_by: blocked_reason(settings),
        problems,
        last_seen: None,
    }
}

pub fn init(settings: &Path) -> InitOutcome {
    if let Some(reason) = blocked_reason(settings) {
        let document = read_settings(settings).ok().flatten();
        let existing = document.as_ref().and_then(statusline_command);
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {reason}"),
        )
        .with_snippet(snippet(existing));
    }
    let mut document = match read_settings(settings) {
        Ok(document) => document.unwrap_or_else(|| json!({})),
        Err(problem) => {
            return InitOutcome::new(
                HARNESS,
                InitResult::Skipped,
                format!("not changed: {problem}; fix it, then run init again"),
            );
        }
    };
    let existing = statusline_command(&document).map(str::to_owned);
    if existing
        .as_deref()
        .is_some_and(|command| command.contains(MARKER))
    {
        return InitOutcome::new(
            HARNESS,
            InitResult::AlreadyInstalled,
            "statusLine already runs the hook",
        );
    }
    let Some(object) = document.as_object_mut() else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: settings is not a JSON object",
        );
    };
    let command = wrapped(existing.as_deref());
    match object.get_mut("statusLine").and_then(Value::as_object_mut) {
        Some(statusline) => {
            statusline.insert("command".into(), Value::String(command.clone()));
        }
        None => {
            object.insert(
                "statusLine".into(),
                json!({ "type": "command", "command": command }),
            );
        }
    }
    let Ok(mut text) = serde_json::to_string_pretty(&document) else {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            "not changed: could not serialize settings",
        );
    };
    text.push('\n');
    let saved = backup(settings).and_then(|saved| write_atomic(settings, &text).map(|()| saved));
    match saved {
        Ok(saved) => InitOutcome::new(
            HARNESS,
            InitResult::Installed,
            match existing {
                Some(_) => "statusLine now runs the hook first, then your existing command",
                None => "statusLine now runs the hook",
            },
        )
        .with_backup(saved),
        Err(error) => InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {error}"),
        ),
    }
}

/// Echoes the JSON onward only when an existing status line will consume it;
/// otherwise the echo would print as the status line itself.
fn wrapped(existing: Option<&str>) -> String {
    match existing {
        None => MARKER.to_owned(),
        Some(command)
            if ["&&", "||", ";", "\n"]
                .iter()
                .any(|op| command.contains(op)) =>
        {
            format!("{MARKER} --passthrough | {{ {command}; }}")
        }
        Some(command) => format!("{MARKER} --passthrough | {command}"),
    }
}

fn snippet(existing: Option<&str>) -> String {
    format!(
        "\"statusLine\": {{ \"type\": \"command\", \"command\": {} }}",
        Value::String(wrapped(existing))
    )
}

fn statusline_command(document: &Value) -> Option<&str> {
    document.get("statusLine")?.get("command")?.as_str()
}

fn read_settings(path: &Path) -> Result<Option<Value>, String> {
    let Some(text) = read_text(path) else {
        return Ok(None);
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("{} is not valid JSON ({error})", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::hooks::files::scratch_dir;
    use crate::hooks::reports::InitResult;

    use super::{init, status};

    fn settings(dir: &std::path::Path, contents: &str) -> std::path::PathBuf {
        let path = dir.join("settings.json");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn a_missing_file_is_not_installed_and_init_creates_a_silent_hook() {
        let dir = scratch_dir();
        let path = dir.join("settings.json");
        assert!(!status(&path).installed);

        let outcome = init(&path);

        assert_eq!(outcome.result, InitResult::Installed);
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["statusLine"]["command"], "mitos hook claude");
        assert!(status(&path).installed);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_existing_status_line_is_wrapped_and_the_rest_is_kept_in_order() {
        let dir = scratch_dir();
        let path = settings(
            &dir,
            r#"{"zeta":1,"statusLine":{"type":"command","command":"pharos statusline scrape","refreshInterval":15},"alpha":2}"#,
        );

        let outcome = init(&path);

        assert_eq!(outcome.result, InitResult::Installed);
        assert!(outcome.backup.is_some());
        let text = fs::read_to_string(&path).unwrap();
        let written: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            written["statusLine"]["command"],
            "mitos hook claude --passthrough | pharos statusline scrape"
        );
        assert_eq!(written["statusLine"]["refreshInterval"], 15);
        assert!(text.find("zeta").unwrap() < text.find("statusLine").unwrap());
        assert!(text.find("statusLine").unwrap() < text.find("alpha").unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_compound_command_is_grouped_so_the_pipe_feeds_all_of_it() {
        let dir = scratch_dir();
        let path = settings(
            &dir,
            r#"{"statusLine":{"type":"command","command":"a && b"}}"#,
        );

        init(&path);

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            written["statusLine"]["command"],
            "mitos hook claude --passthrough | { a && b; }"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_twice_changes_nothing_the_second_time() {
        let dir = scratch_dir();
        let path = settings(&dir, r#"{"statusLine":{"type":"command","command":"x"}}"#);
        init(&path);
        let after_first = fs::read_to_string(&path).unwrap();

        let outcome = init(&path);

        assert_eq!(outcome.result, InitResult::AlreadyInstalled);
        assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_is_reported_and_never_overwritten() {
        let dir = scratch_dir();
        let path = settings(&dir, "{ not json");

        assert!(!status(&path).problems.is_empty());
        let outcome = init(&path);

        assert_eq!(outcome.result, InitResult::Skipped);
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_read_only_file_is_reported_and_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir();
        let path = settings(&dir, r#"{"a":1}"#);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();

        assert!(status(&path).blocked_by.is_some());
        let outcome = init(&path);

        assert_eq!(outcome.result, InitResult::Skipped);
        assert!(outcome.snippet.is_some());
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"a":1}"#);
        fs::remove_dir_all(dir).unwrap();
    }
}
