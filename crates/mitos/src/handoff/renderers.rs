use std::fmt::Write as _;

use anyhow::Result;

use super::{HandoffFacts, HandoffRenderer};
use crate::domain::{EventKind, HarnessKind, ThreadId};

pub const PREAMBLE_PREFIX: &str = "You are joining Mitos session ";

const PRUNED_NOTICE: &str = "- [Earlier tool activity omitted to fit the context limit.]";

/// The file list is never pruned, so it is capped instead.
const MAX_LISTED_FILES: usize = 100;

pub struct DeterministicRenderer;

impl HandoffRenderer for DeterministicRenderer {
    fn render(&self, facts: &HandoffFacts, max_bytes: usize) -> Result<String> {
        let mut sections = Sections::collect(facts);
        let full = sections.assemble(&facts.thread_id, false);
        if full.len() <= max_bytes {
            return Ok(full);
        }
        let excess = full.len() + PRUNED_NOTICE.len() + 1 - max_bytes;
        if !sections.prune(excess) {
            return Ok(full);
        }
        Ok(sections.assemble(&facts.thread_id, true))
    }
}

/// What gets dropped first; messages, notes and the sections above the
/// conversation are never pruned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    ToolResult,
    Activity,
}

struct Line {
    text: String,
    tier: Option<Tier>,
}

#[derive(Default)]
struct Sections {
    summary: Option<String>,
    decisions: Vec<String>,
    questions: Vec<String>,
    files: Vec<String>,
    conversation: Vec<Line>,
}

impl Sections {
    fn collect(facts: &HandoffFacts) -> Self {
        let mut sections = Self {
            files: facts.files_touched.clone(),
            ..Self::default()
        };
        for event in &facts.events {
            let Some(content) = &event.content else {
                continue;
            };
            let (text, tier) = match event.kind {
                EventKind::Compaction => {
                    sections.summary = Some(content.clone());
                    continue;
                }
                EventKind::Decision => {
                    sections.decisions.push(content.clone());
                    continue;
                }
                EventKind::Question => {
                    sections.questions.push(content.clone());
                    continue;
                }
                EventKind::UserMessage | EventKind::AssistantMessage => {
                    let role = event.role.as_deref().unwrap_or("participant");
                    (format!("- {role}: {content}"), None)
                }
                EventKind::Note => (
                    format!(
                        "- Note from {}: {content}",
                        event.harness.map_or("Mitos", HarnessKind::as_str)
                    ),
                    None,
                ),
                EventKind::ToolResult => (
                    format!("- {}: {content}", event.kind.as_str()),
                    Some(Tier::ToolResult),
                ),
                EventKind::ToolCall | EventKind::Status | EventKind::Usage | EventKind::Error => (
                    format!("- {}: {content}", event.kind.as_str()),
                    Some(Tier::Activity),
                ),
                _ => continue,
            };
            sections.conversation.push(Line { text, tier });
        }
        sections
    }

    fn assemble(&self, thread_id: &ThreadId, pruned: bool) -> String {
        let mut output = format!(
            "{PREAMBLE_PREFIX}{thread_id}. The conversation so far follows; reply to the final user message.\n\n"
        );
        if let Some(summary) = &self.summary {
            writeln!(output, "Summary of the earlier conversation:\n{summary}\n")
                .expect("writing to String cannot fail");
        }
        push_list(&mut output, "Decisions:", &self.decisions);
        push_list(&mut output, "Open questions:", &self.questions);
        push_files(&mut output, &self.files);
        if pruned || !self.conversation.is_empty() {
            output.push_str("Conversation:\n");
        }
        if pruned {
            writeln!(output, "{PRUNED_NOTICE}").expect("writing to String cannot fail");
        }
        for line in &self.conversation {
            writeln!(output, "{}", line.text).expect("writing to String cannot fail");
        }
        output
    }

    /// Drops the oldest lines of each tier, cheapest first, until `excess`
    /// bytes are gone. Returns whether anything was dropped.
    fn prune(&mut self, mut excess: usize) -> bool {
        let mut pruned = false;
        for tier in [Tier::ToolResult, Tier::Activity] {
            self.conversation.retain(|line| {
                if excess == 0 || line.tier != Some(tier) {
                    return true;
                }
                excess = excess.saturating_sub(line.text.len() + 1);
                pruned = true;
                false
            });
        }
        pruned
    }
}

fn push_list(output: &mut String, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    writeln!(output, "{title}").expect("writing to String cannot fail");
    for item in items {
        writeln!(output, "- {item}").expect("writing to String cannot fail");
    }
    output.push('\n');
}

fn push_files(output: &mut String, files: &[String]) {
    push_list(
        output,
        "Files touched:",
        &files[..files.len().min(MAX_LISTED_FILES)],
    );
    if files.len() > MAX_LISTED_FILES {
        // push_list ended the list with a blank line; reopen it for the tail.
        output.truncate(output.len() - 1);
        writeln!(output, "- … and {} more\n", files.len() - MAX_LISTED_FILES)
            .expect("writing to String cannot fail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ThreadEvent;

    fn event(kind: EventKind, role: Option<&str>, content: &str) -> ThreadEvent {
        ThreadEvent {
            thread_id: "t1".into(),
            seq: 1,
            turn_id: None,
            harness: None,
            kind,
            role: role.map(Into::into),
            content: Some(content.into()),
            payload: None,
            created_at: "".into(),
        }
    }

    fn facts(events: Vec<ThreadEvent>) -> HandoffFacts {
        HandoffFacts {
            thread_id: "t1".into(),
            events,
            files_touched: Vec::new(),
        }
    }

    fn render(events: Vec<ThreadEvent>, max_bytes: usize) -> String {
        DeterministicRenderer
            .render(&facts(events), max_bytes)
            .unwrap()
    }

    #[test]
    fn files_touched_are_listed_between_the_questions_and_the_conversation() {
        let mut with_files = facts(vec![
            event(EventKind::Question, None, "why"),
            event(EventKind::UserMessage, Some("user"), "go"),
        ]);
        with_files.files_touched = vec!["src/a.rs".into(), "b.rs".into()];
        let text = DeterministicRenderer
            .render(&with_files, usize::MAX)
            .unwrap();
        let position = |needle: &str| text.find(needle).unwrap();
        assert!(position("Open questions:") < position("Files touched:\n- src/a.rs\n- b.rs\n\n"));
        assert!(position("Files touched:") < position("Conversation:"));
    }

    #[test]
    fn the_file_list_is_capped_with_a_count_of_the_rest() {
        let mut many = facts(vec![event(EventKind::UserMessage, Some("user"), "go")]);
        many.files_touched = (0..MAX_LISTED_FILES + 7)
            .map(|n| format!("f{n}.rs"))
            .collect();
        let text = DeterministicRenderer.render(&many, usize::MAX).unwrap();
        assert!(text.contains(&format!("- f{}.rs\n", MAX_LISTED_FILES - 1)));
        assert!(!text.contains(&format!("- f{MAX_LISTED_FILES}.rs")));
        assert!(text.contains("- … and 7 more\n\nConversation:"));

        let exact = (0..MAX_LISTED_FILES).map(|n| format!("f{n}.rs")).collect();
        many.files_touched = exact;
        let text = DeterministicRenderer.render(&many, usize::MAX).unwrap();
        assert!(!text.contains("more"));
    }

    #[test]
    fn render_ends_with_latest_message_and_has_no_directive() {
        let text = render(
            vec![event(
                EventKind::UserMessage,
                Some("user"),
                "Tell me something",
            )],
            usize::MAX,
        );
        assert!(text.trim_end().ends_with("- user: Tell me something"));
        assert!(!text.contains("Inspect the workspace"));
    }

    #[test]
    fn summary_decisions_and_questions_get_their_own_sections_ahead_of_the_conversation() {
        let text = render(
            vec![
                event(EventKind::Compaction, None, "We chose libSQL."),
                event(EventKind::Decision, None, "Branch events share the log"),
                event(EventKind::Question, None, "Who judges the winner?"),
                event(EventKind::UserMessage, Some("user"), "continue"),
            ],
            usize::MAX,
        );
        let position = |needle: &str| text.find(needle).unwrap();
        assert!(
            position("Summary of the earlier conversation:\nWe chose libSQL.")
                < position("Decisions:\n- Branch events share the log")
        );
        assert!(position("Decisions:") < position("Open questions:\n- Who judges the winner?"));
        assert!(position("Open questions:") < position("Conversation:\n- user: continue"));
        assert_eq!(text.matches("Branch events share the log").count(), 1);
    }

    #[test]
    fn empty_sections_are_left_out() {
        let text = render(
            vec![event(EventKind::UserMessage, Some("user"), "hi")],
            usize::MAX,
        );
        assert!(!text.contains("Summary of"));
        assert!(!text.contains("Decisions:"));
        assert!(!text.contains("Open questions:"));
        assert!(text.contains("Conversation:\n- user: hi"));
        assert!(!render(Vec::new(), usize::MAX).contains("Conversation:"));
    }

    #[test]
    fn events_without_content_or_a_rendered_kind_are_skipped() {
        let mut silent = event(EventKind::AssistantMessage, Some("assistant"), "x");
        silent.content = None;
        let text = render(
            vec![
                silent,
                event(EventKind::HarnessBound, None, "bound"),
                event(EventKind::Note, None, "remember this"),
            ],
            usize::MAX,
        );
        assert!(!text.contains("bound"));
        assert!(text.contains("- Note from Mitos: remember this"));
    }

    #[test]
    fn nothing_is_pruned_while_the_handoff_fits() {
        let events = vec![
            event(EventKind::ToolCall, None, "Edit"),
            event(EventKind::ToolResult, None, "ok"),
            event(EventKind::UserMessage, Some("user"), "hi"),
        ];
        let full = render(events.clone(), usize::MAX);
        assert_eq!(render(events, full.len()), full);
        assert!(full.contains("- tool_result: ok"));
        assert!(!full.contains("omitted"));
    }

    #[test]
    fn tool_results_go_first_oldest_first_and_messages_survive() {
        let big = "x".repeat(400);
        let events = vec![
            event(EventKind::UserMessage, Some("user"), "first question"),
            event(EventKind::ToolCall, None, "Read"),
            event(EventKind::ToolResult, None, &format!("old {big}")),
            event(EventKind::ToolResult, None, &format!("new {big}")),
            event(EventKind::AssistantMessage, Some("assistant"), "answer"),
        ];
        let full = render(events.clone(), usize::MAX);
        let text = render(events, full.len() - 100);
        assert!(text.len() <= full.len() - 100);
        assert!(!text.contains("old xxx"));
        assert!(text.contains("new xxx"));
        assert!(text.contains("- tool_call: Read"));
        assert!(text.contains("tool activity omitted"));
        assert!(text.contains("- user: first question"));
        assert!(text.trim_end().ends_with("- assistant: answer"));
    }

    #[test]
    fn tool_calls_and_status_go_only_after_every_tool_result() {
        let big = "x".repeat(400);
        let events = vec![
            event(EventKind::ToolCall, None, &format!("call {big}")),
            event(EventKind::Status, None, &format!("status {big}")),
            event(EventKind::ToolResult, None, "tiny"),
            event(EventKind::UserMessage, Some("user"), "ask"),
        ];
        let text = render(events, 300);
        assert!(!text.contains("tiny"));
        assert!(!text.contains("call xxx"));
        assert!(!text.contains("status xxx"));
        assert!(text.contains("- user: ask"));
    }

    #[test]
    fn messages_are_never_pruned_even_when_they_alone_exceed_the_budget() {
        let events = vec![
            event(EventKind::UserMessage, Some("user"), &"a".repeat(500)),
            event(EventKind::ToolResult, None, "tool"),
        ];
        let text = render(events, 100);
        assert!(text.contains(&"a".repeat(500)));
        assert!(!text.contains("- tool_result"));

        let only_messages = vec![event(
            EventKind::UserMessage,
            Some("user"),
            &"b".repeat(500),
        )];
        let text = render(only_messages, 100);
        assert!(text.contains(&"b".repeat(500)));
        assert!(!text.contains("omitted"));
    }
}
