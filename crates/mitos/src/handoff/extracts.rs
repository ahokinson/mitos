use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use crate::domain::{EventKind, ThreadEvent};

const EDIT_TOOLS: [&str; 4] = ["edit", "write", "multiedit", "notebookedit"];

/// Paths the harnesses reported editing or writing, relative to the workspace
/// and in the order first seen. Reads the four harnesses' tool payloads as
/// they are recorded; a failed edit still counts as touched.
pub fn files_touched(events: &[ThreadEvent], workspace_root: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for event in events {
        if !matches!(event.kind, EventKind::ToolCall | EventKind::ToolResult) {
            continue;
        }
        let Some(payload) = &event.payload else {
            continue;
        };
        for path in payload_paths(payload) {
            let path = relative(path, workspace_root);
            if seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }
    files
}

pub fn contents_of(events: &[ThreadEvent], kind: EventKind) -> Vec<String> {
    events
        .iter()
        .filter(|event| event.kind == kind)
        .filter_map(|event| event.content.clone())
        .collect()
}

fn payload_paths(payload: &Value) -> Vec<&str> {
    let text = |pointer: &str| payload.pointer(pointer).and_then(Value::as_str);
    let is_edit = |pointer: &str| {
        text(pointer).is_some_and(|name| EDIT_TOOLS.contains(&name.to_lowercase().as_str()))
    };
    match payload.get("type").and_then(Value::as_str) {
        Some("tool_use") if is_edit("/name") => text("/input/file_path")
            .or_else(|| text("/input/notebook_path"))
            .into_iter()
            .collect(),
        Some("tool_result") => text("/tool_use_result/filePath").into_iter().collect(),
        Some("fileChange") => entries(payload, "changes", None),
        Some("tool") if is_edit("/tool") => text("/state/input/filePath")
            .or_else(|| text("/state/input/path"))
            .into_iter()
            .collect(),
        None | Some(_) => entries(payload, "content", Some("diff")),
    }
}

/// The `path` of each array entry under `key`, limited to entries of the
/// given `type` when one is named.
fn entries<'a>(payload: &'a Value, key: &str, kind: Option<&str>) -> Vec<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            kind.is_none_or(|kind| entry.get("type").and_then(Value::as_str) == Some(kind))
        })
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .collect()
}

fn relative(path: &str, workspace_root: &str) -> String {
    Path::new(path)
        .strip_prefix(workspace_root)
        .map_or_else(|_| path.to_owned(), |rest| rest.display().to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(kind: EventKind, payload: Option<Value>, content: Option<&str>) -> ThreadEvent {
        ThreadEvent {
            thread_id: "t1".into(),
            seq: 1,
            turn_id: None,
            harness: None,
            kind,
            role: None,
            content: content.map(Into::into),
            payload,
            created_at: String::new(),
        }
    }

    fn call(payload: Value) -> ThreadEvent {
        event(EventKind::ToolCall, Some(payload), None)
    }

    #[test]
    fn claude_edits_and_writes_are_files_touched_but_reads_and_commands_are_not() {
        let events = [
            call(
                json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "/w/src/a.rs" } }),
            ),
            call(
                json!({ "type": "tool_use", "name": "Write", "input": { "file_path": "/w/b.rs" } }),
            ),
            call(
                json!({ "type": "tool_use", "name": "NotebookEdit", "input": { "notebook_path": "/w/n.ipynb" } }),
            ),
            call(
                json!({ "type": "tool_use", "name": "Read", "input": { "file_path": "/w/read.rs" } }),
            ),
            call(json!({ "type": "tool_use", "name": "Bash", "input": { "command": "ls" } })),
        ];
        assert_eq!(
            files_touched(&events, "/w"),
            ["src/a.rs", "b.rs", "n.ipynb"]
        );
    }

    #[test]
    fn a_claude_result_names_the_file_it_changed() {
        let events = [event(
            EventKind::ToolResult,
            Some(json!({ "type": "tool_result", "tool_use_result": { "filePath": "/w/c.rs" } })),
            None,
        )];
        assert_eq!(files_touched(&events, "/w"), ["c.rs"]);
    }

    #[test]
    fn codex_file_changes_list_every_path() {
        let events = [call(json!({
            "type": "fileChange",
            "changes": [{ "path": "/w/x.rs", "diff": "" }, { "path": "/w/y.rs", "diff": "" }]
        }))];
        assert_eq!(files_touched(&events, "/w"), ["x.rs", "y.rs"]);
    }

    #[test]
    fn opencode_edit_and_write_tools_use_the_file_path_or_path_input() {
        let events = [
            call(
                json!({ "type": "tool", "tool": "edit", "state": { "input": { "filePath": "/w/o.ts" } } }),
            ),
            call(
                json!({ "type": "tool", "tool": "write", "state": { "input": { "path": "/w/p.ts" } } }),
            ),
            call(
                json!({ "type": "tool", "tool": "bash", "state": { "input": { "command": "ls" } } }),
            ),
        ];
        assert_eq!(files_touched(&events, "/w"), ["o.ts", "p.ts"]);
    }

    #[test]
    fn hermes_diff_blocks_are_files_touched_but_text_blocks_are_not() {
        let events = [call(json!({
            "content": [
                { "type": "diff", "path": "/w/h.py", "oldText": "", "newText": "x" },
                { "type": "content", "path": "/w/ignored.py" }
            ]
        }))];
        assert_eq!(files_touched(&events, "/w"), ["h.py"]);
    }

    #[test]
    fn paths_repeat_once_and_outside_the_workspace_stay_as_reported() {
        let events = [
            call(
                json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "/w/a.rs" } }),
            ),
            call(
                json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "/w/a.rs" } }),
            ),
            call(
                json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "/elsewhere/z.rs" } }),
            ),
            call(json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "rel.rs" } })),
        ];
        assert_eq!(
            files_touched(&events, "/w"),
            ["a.rs", "/elsewhere/z.rs", "rel.rs"]
        );
    }

    #[test]
    fn events_without_a_payload_or_of_other_kinds_are_ignored() {
        let events = [
            event(EventKind::ToolCall, None, Some("Edit")),
            event(
                EventKind::AssistantMessage,
                Some(
                    json!({ "type": "tool_use", "name": "Edit", "input": { "file_path": "/w/q.rs" } }),
                ),
                None,
            ),
        ];
        assert!(files_touched(&events, "/w").is_empty());
    }

    #[test]
    fn contents_of_returns_the_text_of_one_kind_in_order() {
        let events = [
            event(EventKind::Decision, None, Some("first")),
            event(EventKind::Question, None, Some("ask")),
            event(EventKind::Decision, None, None),
            event(EventKind::Decision, None, Some("second")),
        ];
        assert_eq!(
            contents_of(&events, EventKind::Decision),
            ["first", "second"]
        );
        assert_eq!(contents_of(&events, EventKind::Question), ["ask"]);
    }
}
