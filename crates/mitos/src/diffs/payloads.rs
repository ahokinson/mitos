use serde_json::Value;

use super::FileDiff;
use super::hunks::{Hunk, diff_from_unified, diff_texts, file_diff_of};
use super::lines::DiffLine;

/// File diffs carried by a tool-call payload, across the four harnesses'
/// shapes. Empty for anything that isn't a file edit.
pub fn payload_diffs(payload: &Value) -> Vec<FileDiff> {
    if !payload.is_object() {
        return Vec::new();
    }
    match payload.get("type").and_then(Value::as_str) {
        Some("tool_use") => Vec::new(),
        Some("tool_result") => claude_result_diffs(payload),
        Some("fileChange") => codex_diffs(payload),
        Some("tool") => opencode_diffs(payload),
        _ => hermes_diffs(payload),
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn text_or_empty<'a>(value: &'a Value, key: &str) -> &'a str {
    text(value, key).unwrap_or_default()
}

fn array<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn object<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|inner| inner.is_object())
}

fn claude_result_diffs(payload: &Value) -> Vec<FileDiff> {
    let Some(result) = object(payload, "tool_use_result") else {
        return Vec::new();
    };
    let Some(path) = text(result, "filePath") else {
        return Vec::new();
    };
    let hunks = array(result, "structuredPatch")
        .filter_map(structured_hunk)
        .collect();
    if let Some(diff) = file_diff_of(path, hunks) {
        return vec![diff];
    }
    let diff = match (text(result, "type"), text(result, "content")) {
        (Some("create"), Some(created)) => diff_texts(path, "", created),
        _ => diff_texts(
            path,
            text_or_empty(result, "oldString"),
            text_or_empty(result, "newString"),
        ),
    };
    diff.into_iter().collect()
}

fn structured_hunk(entry: &Value) -> Option<Hunk<'_>> {
    if !entry.is_object() {
        return None;
    }
    let lines: Vec<DiffLine<'_>> = entry
        .get("lines")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(DiffLine::parse)
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(Hunk {
        old_start: start_line(entry.get("oldStart")),
        new_start: start_line(entry.get("newStart")),
        lines,
    })
}

/// A missing, zero or non-numeric start counts as line 1.
fn start_line(value: Option<&Value>) -> usize {
    value
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|number| usize::try_from(number).ok())
                .or_else(|| value.as_str()?.parse().ok())
        })
        .filter(|&line| line > 0)
        .unwrap_or(1)
}

fn codex_diffs(payload: &Value) -> Vec<FileDiff> {
    array(payload, "changes")
        .filter_map(|change| {
            let path = text(change, "path")?;
            let body = text(change, "diff")?;
            let kind = object(change, "kind")
                .and_then(|kind| text(kind, "type"))
                .or_else(|| text(change, "kind"));
            if body.contains("\n@@") || body.starts_with("@@") {
                diff_from_unified(path, body)
            } else if kind == Some("delete") {
                diff_texts(path, body, "")
            } else {
                diff_texts(path, "", body)
            }
        })
        .collect()
}

fn opencode_diffs(payload: &Value) -> Vec<FileDiff> {
    let Some(state) = object(payload, "state") else {
        return Vec::new();
    };
    let Some(input) = object(state, "input") else {
        return Vec::new();
    };
    let Some(path) = text(input, "filePath").or_else(|| text(input, "path")) else {
        return Vec::new();
    };
    let unified = object(state, "metadata")
        .and_then(|metadata| text(metadata, "diff"))
        .filter(|body| !body.is_empty());
    if let Some(diff) = unified.and_then(|body| diff_from_unified(path, body)) {
        return vec![diff];
    }
    let diff = match text(payload, "tool") {
        Some("write") => diff_texts(path, "", text_or_empty(input, "content")),
        Some("edit") => diff_texts(
            path,
            text_or_empty(input, "oldString"),
            text_or_empty(input, "newString"),
        ),
        _ => None,
    };
    diff.into_iter().collect()
}

fn hermes_diffs(payload: &Value) -> Vec<FileDiff> {
    array(payload, "content")
        .filter_map(|block| {
            let path = text(block, "path")?;
            if text(block, "type") != Some("diff") {
                return None;
            }
            diff_texts(
                path,
                text_or_empty(block, "oldText"),
                text_or_empty(block, "newText"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn summary(diffs: &[FileDiff]) -> Vec<(&str, usize, usize)> {
        diffs
            .iter()
            .map(|diff| (diff.path.as_str(), diff.added, diff.removed))
            .collect()
    }

    #[test]
    fn claude_tool_result_uses_the_structured_patch_with_real_line_numbers() {
        let diffs = payload_diffs(&json!({
            "type": "tool_result",
            "tool_use_result": {
                "filePath": "/p/a.ts",
                "structuredPatch": [
                    { "oldStart": 10, "newStart": 10, "lines": [" ctx", "-x", "+y", " ctx"] }
                ]
            }
        }));
        assert_eq!(summary(&diffs), [("/p/a.ts", 1, 1)]);
        assert!(diffs[0].diff.contains("@@ -10,3 +10,3 @@"));
    }

    #[test]
    fn claude_tool_result_falls_back_to_old_new_strings_and_create_content() {
        let edit = payload_diffs(&json!({
            "type": "tool_result",
            "tool_use_result": { "filePath": "/p/a.ts", "oldString": "x", "newString": "y" }
        }));
        assert!(edit[0].diff.contains("-x\n+y"));

        let write = payload_diffs(&json!({
            "type": "tool_result",
            "tool_use_result": {
                "type": "create",
                "filePath": "/p/w.ts",
                "content": "hello\n",
                "structuredPatch": []
            }
        }));
        assert_eq!(write[0].added, 1);
    }

    #[test]
    fn claude_tool_use_blocks_and_non_edit_results_yield_nothing() {
        let tool_use = json!({
            "type": "tool_use",
            "name": "Edit",
            "input": { "file_path": "/p/a.ts", "old_string": "x", "new_string": "y" }
        });
        assert!(payload_diffs(&tool_use).is_empty());
        assert!(payload_diffs(&json!({ "type": "tool_result", "content": "ok" })).is_empty());
        let shell = json!({
            "type": "tool_result",
            "tool_use_result": { "stdout": "a", "stderr": "" }
        });
        assert!(payload_diffs(&shell).is_empty());
    }

    #[test]
    fn codex_file_changes_use_unified_diffs_and_raw_content_for_add_and_delete() {
        let diffs = payload_diffs(&json!({
            "type": "fileChange",
            "changes": [
                {
                    "path": "a.ts",
                    "kind": { "type": "update" },
                    "diff": "@@ -1,2 +1,2 @@\n keep\n-old\n+new\n"
                },
                { "path": "b.ts", "kind": { "type": "add" }, "diff": "fresh\n" },
                { "path": "c.ts", "kind": { "type": "delete" }, "diff": "gone\n" }
            ]
        }));
        assert_eq!(
            summary(&diffs),
            [("a.ts", 1, 1), ("b.ts", 1, 0), ("c.ts", 0, 1)]
        );
    }

    #[test]
    fn codex_accepts_a_plain_string_kind() {
        let diffs = payload_diffs(&json!({
            "type": "fileChange",
            "changes": [{ "path": "c.ts", "kind": "delete", "diff": "gone\n" }]
        }));
        assert_eq!(summary(&diffs), [("c.ts", 0, 1)]);
    }

    #[test]
    fn opencode_prefers_metadata_diff_and_falls_back_to_input() {
        let from_metadata = payload_diffs(&json!({
            "type": "tool",
            "tool": "edit",
            "state": {
                "input": { "filePath": "a.ts", "oldString": "x", "newString": "y" },
                "metadata": { "diff": "@@ -1 +1 @@\n-x\n+y\n" }
            }
        }));
        assert!(from_metadata[0].diff.contains("@@ -1,1 +1,1 @@"));

        let from_input = payload_diffs(&json!({
            "type": "tool",
            "tool": "edit",
            "state": { "input": { "filePath": "a.ts", "oldString": "x", "newString": "y" } }
        }));
        assert_eq!(from_input[0].added, 1);

        let write = payload_diffs(&json!({
            "type": "tool",
            "tool": "write",
            "state": { "input": { "filePath": "n.ts", "content": "a\nb" } }
        }));
        assert_eq!(write[0].added, 2);

        let shell = json!({
            "type": "tool",
            "tool": "bash",
            "state": { "input": { "command": "ls" } }
        });
        assert!(payload_diffs(&shell).is_empty());
    }

    #[test]
    fn hermes_diff_blocks_yield_diffs_and_a_null_old_text_means_a_new_file() {
        let diffs = payload_diffs(&json!({
            "toolCallId": "t1",
            "content": [
                { "type": "content", "content": { "type": "text", "text": "hi" } },
                { "type": "diff", "path": "a.ts", "oldText": "x\n", "newText": "y\n" },
                { "type": "diff", "path": "n.ts", "oldText": null, "newText": "z\n" }
            ]
        }));
        assert_eq!(summary(&diffs), [("a.ts", 1, 1), ("n.ts", 1, 0)]);
    }

    #[test]
    fn unrecognised_payloads_yield_nothing() {
        for payload in [json!(null), json!("text"), json!([]), json!({})] {
            assert!(payload_diffs(&payload).is_empty());
        }
    }
}
