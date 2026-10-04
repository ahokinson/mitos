use serde::Serialize;
use serde_json::Value;

use crate::domain::{EventKind, ToolKind};

const ARGUMENT_KEYS: [&str; 9] = [
    "command",
    "file_path",
    "notebook_path",
    "pattern",
    "path",
    "url",
    "query",
    "description",
    "prompt",
];

/// Which of the payload's own shapes an event carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolShape {
    ToolUse,
    ToolResult,
    Other,
}

/// What a tool event's payload says, normalized across harnesses so a frontend
/// never reads the payload itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolFacts {
    pub kind: ToolKind,
    /// The tool's own name, when the harness reports one separately.
    pub name: Option<String>,
    /// The call's main argument in full; for harnesses that describe the call
    /// in `content`, that text.
    pub argument: String,
    /// The id that ties a call to its result.
    pub tool_use_id: Option<String>,
    /// The file a call reads or edits.
    pub path: Option<String>,
    pub shape: ToolShape,
    /// A successful read or file edit, whose result adds nothing to its call.
    pub quiet: bool,
    /// A successful result that carries a patch.
    pub patched: bool,
}

pub fn tool_facts(
    kind: EventKind,
    payload: Option<&Value>,
    content: Option<&str>,
) -> Option<ToolFacts> {
    if !matches!(kind, EventKind::ToolCall | EventKind::ToolResult) {
        return None;
    }
    let payload = payload.unwrap_or(&Value::Null);
    let shape = match text(payload, "type") {
        Some("tool_use") => ToolShape::ToolUse,
        Some("tool_result") => ToolShape::ToolResult,
        _ => ToolShape::Other,
    };
    let tool_use_id = match shape {
        ToolShape::ToolUse => text(payload, "id"),
        _ => text(payload, "tool_use_id"),
    }
    .map(str::to_owned);

    let result = payload.get("tool_use_result");
    let has = |key: &str| result.is_some_and(|result| result.get(key).is_some());
    let succeeded =
        shape == ToolShape::ToolResult && payload.get("is_error") != Some(&Value::Bool(true));
    let quiet = succeeded && (has("file") || has("structuredPatch"));
    let patched = succeeded && tool_use_id.is_some() && has("structuredPatch");

    let call = if kind == EventKind::ToolCall {
        call_fields(payload, shape, content)
    } else {
        CallFields::default()
    };
    Some(ToolFacts {
        kind: call.kind,
        name: call.name,
        argument: call.argument,
        tool_use_id,
        path: call.path,
        shape,
        quiet,
        patched,
    })
}

struct CallFields {
    kind: ToolKind,
    name: Option<String>,
    argument: String,
    path: Option<String>,
}

impl Default for CallFields {
    fn default() -> Self {
        Self {
            kind: ToolKind::Other,
            name: None,
            argument: String::new(),
            path: None,
        }
    }
}

fn call_fields(payload: &Value, shape: ToolShape, content: Option<&str>) -> CallFields {
    if shape == ToolShape::ToolUse {
        let input = payload.get("input");
        let argument = ARGUMENT_KEYS
            .iter()
            .find_map(|key| input?.get(key)?.as_str().filter(|text| !text.is_empty()))
            .unwrap_or_default();
        return CallFields {
            kind: text(payload, "name").map_or(ToolKind::Other, ToolKind::named),
            name: Some(content.unwrap_or_default().to_owned()),
            argument: argument.to_owned(),
            path: input
                .and_then(|input| text(input, "file_path"))
                .map(str::to_owned),
        };
    }
    let kind = ["tool", "kind", "type"]
        .iter()
        .filter_map(|key| text(payload, key))
        .map(ToolKind::named)
        .find(|kind| *kind != ToolKind::Other)
        .unwrap_or(ToolKind::Other);
    CallFields {
        kind,
        argument: content.unwrap_or_default().to_owned(),
        ..CallFields::default()
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn call(content: &str, payload: &Value) -> ToolFacts {
        tool_facts(EventKind::ToolCall, Some(payload), Some(content)).unwrap()
    }

    fn claude(name: &str, input: &Value) -> ToolFacts {
        call(
            name,
            &json!({ "type": "tool_use", "name": name, "input": input }),
        )
    }

    fn result(payload: &Value) -> ToolFacts {
        tool_facts(EventKind::ToolResult, Some(payload), Some("ok")).unwrap()
    }

    #[test]
    fn non_tool_events_have_no_facts() {
        assert_eq!(tool_facts(EventKind::UserMessage, None, Some("hi")), None);
        assert_eq!(tool_facts(EventKind::Status, Some(&json!({})), None), None);
    }

    #[test]
    fn a_claude_call_takes_its_kind_from_the_tool_and_its_argument_from_the_input() {
        let shell = claude(
            "Bash",
            &json!({ "command": "git status", "description": "x" }),
        );
        assert_eq!(shell.kind, ToolKind::Shell);
        assert_eq!(shell.name.as_deref(), Some("Bash"));
        assert_eq!(shell.argument, "git status");
        assert_eq!(shell.shape, ToolShape::ToolUse);

        assert_eq!(
            claude("Read", &json!({ "file_path": "src/a.ts", "limit": 5 })).argument,
            "src/a.ts"
        );
        assert_eq!(
            claude("Grep", &json!({ "pattern": "foo", "path": "src" })).argument,
            "foo"
        );
        let todo = claude("TodoWrite", &json!({ "todos": [] }));
        assert_eq!((todo.kind, todo.argument.as_str()), (ToolKind::Todo, ""));
    }

    #[test]
    fn an_empty_argument_value_is_skipped_for_the_next_key() {
        let facts = claude("Bash", &json!({ "command": "", "description": "list" }));
        assert_eq!(facts.argument, "list");
    }

    #[test]
    fn a_claude_call_reports_its_file_and_id() {
        let facts = call(
            "Edit",
            &json!({ "type": "tool_use", "id": "e1", "name": "Edit", "input": { "file_path": "a.ts" } }),
        );
        assert_eq!(facts.path.as_deref(), Some("a.ts"));
        assert_eq!(facts.tool_use_id.as_deref(), Some("e1"));
    }

    #[test]
    fn other_harnesses_keep_their_content_and_infer_the_kind_from_the_payload() {
        let shell = call("bun test", &json!({ "type": "commandExecution" }));
        assert_eq!(shell.kind, ToolKind::Shell);
        assert_eq!(shell.name, None);
        assert_eq!(shell.argument, "bun test");
        assert_eq!(shell.shape, ToolShape::Other);

        assert_eq!(
            call("1 file change", &json!({ "type": "fileChange" })).kind,
            ToolKind::Edit
        );
        assert_eq!(
            call("read: a.ts", &json!({ "tool": "read" })).kind,
            ToolKind::Read
        );
        assert_eq!(
            call("ls", &json!({ "kind": "execute" })).kind,
            ToolKind::Shell
        );
        assert_eq!(call("mystery", &json!({})).kind, ToolKind::Other);
    }

    #[test]
    fn a_call_without_a_payload_still_has_facts() {
        let facts = tool_facts(EventKind::ToolCall, None, Some("ran")).unwrap();
        assert_eq!(
            (facts.kind, facts.argument.as_str()),
            (ToolKind::Other, "ran")
        );
    }

    #[test]
    fn a_call_and_its_result_share_a_tool_use_id() {
        let use_id = |payload: &Value| call("x", payload).tool_use_id;
        assert_eq!(
            use_id(&json!({ "type": "tool_use", "id": "b1" })).as_deref(),
            Some("b1")
        );
        assert_eq!(
            use_id(&json!({ "type": "tool_result", "tool_use_id": "b1" })).as_deref(),
            Some("b1")
        );
        assert_eq!(use_id(&json!({ "type": "tool_use", "id": 5 })), None);
        assert_eq!(use_id(&json!(null)), None);
    }

    #[test]
    fn only_successful_file_results_are_quiet() {
        assert!(!result(&json!({ "type": "tool_result" })).quiet);
        assert!(result(&json!({ "type": "tool_result", "tool_use_result": { "file": {} } })).quiet);
        assert!(
            result(&json!({ "type": "tool_result", "tool_use_result": { "structuredPatch": [] } }))
                .quiet
        );
        let failed = json!({
            "type": "tool_result",
            "is_error": true,
            "tool_use_result": { "file": {} }
        });
        assert!(!result(&failed).quiet);
        assert!(!call("x", &json!({ "type": "tool_use" })).quiet);
    }

    #[test]
    fn a_patched_result_is_successful_identified_and_carries_a_patch() {
        let patched = |payload: Value| result(&payload).patched;
        let base = json!({
            "type": "tool_result",
            "tool_use_id": "e1",
            "tool_use_result": { "filePath": "a.ts", "structuredPatch": [] }
        });
        assert!(patched(base.clone()));

        let mut failed = base.clone();
        failed["is_error"] = json!(true);
        assert!(!patched(failed));

        let mut anonymous = base;
        anonymous.as_object_mut().unwrap().remove("tool_use_id");
        assert!(!patched(anonymous));

        let read_only = json!({
            "type": "tool_result",
            "tool_use_id": "r1",
            "tool_use_result": { "file": {} }
        });
        assert!(!patched(read_only));
    }
}
