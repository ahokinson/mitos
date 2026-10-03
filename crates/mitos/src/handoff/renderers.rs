use std::fmt::Write as _;

use anyhow::Result;

use super::{HandoffFacts, HandoffRenderer};
use crate::domain::{EventKind, ThreadEvent};

pub struct DeterministicRenderer;

impl HandoffRenderer for DeterministicRenderer {
    fn render(&self, facts: &HandoffFacts) -> Result<String> {
        let mut output = format!(
            "You are joining Mitos session {}. The conversation so far follows; reply to the final user message.\n\n",
            facts.thread_id
        );
        append_event_facts(&mut output, &facts.events);
        Ok(output)
    }
}

fn append_event_facts(output: &mut String, events: &[ThreadEvent]) {
    for event in events {
        let Some(content) = &event.content else {
            continue;
        };
        let line = match event.kind {
            EventKind::UserMessage | EventKind::AssistantMessage => {
                let role = event.role.as_deref().unwrap_or("participant");
                format!("- {role}: {content}")
            }
            EventKind::Note => format!(
                "- Note from {}: {content}",
                event.harness.as_deref().unwrap_or("Mitos")
            ),
            EventKind::Decision => format!("- Decision: {content}"),
            EventKind::Question => format!("- Open question: {content}"),
            EventKind::ToolCall
            | EventKind::ToolResult
            | EventKind::Status
            | EventKind::Usage
            | EventKind::Error => format!("- {}: {content}", event.kind.as_str()),
            _ => continue,
        };
        writeln!(output, "{line}").expect("writing to String cannot fail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_ends_with_latest_message_and_has_no_directive() {
        let event = ThreadEvent {
            thread_id: "t1".into(),
            seq: 1,
            turn_id: None,
            harness: None,
            kind: EventKind::UserMessage,
            role: Some("user".into()),
            content: Some("Tell me something".into()),
            payload: None,
            created_at: String::new(),
        };
        let facts = HandoffFacts {
            thread_id: "t1".into(),
            events: vec![event],
        };
        let text = DeterministicRenderer.render(&facts).unwrap();
        assert!(text.trim_end().ends_with("- user: Tell me something"));
        assert!(!text.contains("Inspect the workspace"));
    }
}
