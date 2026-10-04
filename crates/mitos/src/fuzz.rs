//! Narrow entry points compiled only for coverage-guided fuzzing.

use serde_json::Value;

use crate::{
    diffs::payload_diffs,
    domain::{EventKind, HarnessKind},
    hooks::parse,
    json::{parsed_container, text_value},
    tools::tool_facts,
};

/// Exercises the parsers that normalize untrusted JSON emitted by harnesses.
pub fn structured_payload(bytes: &[u8]) {
    let Ok(payload) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };

    let _ = payload_diffs(&payload);
    let _ = parsed_container(&payload);
    let _ = text_value(&payload);
    let _ = tool_facts(EventKind::ToolCall, Some(&payload), None);
    let _ = tool_facts(EventKind::ToolResult, Some(&payload), None);
    for harness in HarnessKind::ALL {
        let _ = parse(harness, None, &payload);
    }
}
