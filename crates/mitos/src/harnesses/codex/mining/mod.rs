mod parser;

use std::path::Path;

use crate::domain::HarnessKind;
use crate::harnesses::mining::{ReportBuilder, SESSIONS_CEILING};
use crate::history::discover_files;
use crate::wire::requests::MiningRequest;

use parser::SessionParser;

/// `sessions_root` is `~/.codex/sessions`; one `rollout-*.jsonl` is one
/// session. Codex transcripts carry no cost, so the mined report
/// contributes counts and tokens only.
pub fn mine_codex(
    sessions_root: &Path,
    request: &MiningRequest,
) -> crate::wire::responses::MiningReport {
    let mut builder = ReportBuilder::new(HarnessKind::Codex.as_str());
    for candidate in discover_files(sessions_root, ".jsonl", SESSIONS_CEILING) {
        let Ok(bytes) = std::fs::read(&candidate.path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if let Some(stats) = SessionParser::parse(&candidate.path, &text, request) {
            builder.absorb(stats);
        }
    }
    builder.finish().attributed(HarnessKind::Codex.as_str())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::*;
    use crate::wire::requests::DEFAULT_TOP;

    fn near(left: f64, right: f64) -> bool {
        (left - right).abs() < f64::EPSILON
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        /// Writes one rollout file under a dated directory.
        fn new(id: &str, lines: &[Value]) -> Self {
            let root = std::env::temp_dir().join(format!("mitos-codex-{}", crate::domain::id()));
            let nested = root.join("2026").join("01");
            std::fs::create_dir_all(&nested).unwrap();
            let body: Vec<String> = lines.iter().map(Value::to_string).collect();
            std::fs::write(nested.join(format!("rollout-{id}.jsonl")), body.join("\n")).unwrap();
            Self { root }
        }

        fn mine(&self, request: &MiningRequest) -> crate::wire::responses::MiningReport {
            mine_codex(&self.root, request)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn session_meta() -> Value {
        json!({
            "type": "session_meta", "timestamp": "2026-01-02T03:04:05Z",
            "payload": { "session_id": "ses_1", "cwd": "/home/work" }
        })
    }

    fn user_line(id: &str, text: &str) -> Value {
        json!({
            "type": "response_item", "timestamp": "2026-01-02T03:04:05Z",
            "payload": {
                "type": "message", "id": id, "role": "user",
                "content": [{ "type": "input_text", "text": text }]
            }
        })
    }

    fn assistant_line(id: &str) -> Value {
        json!({
            "type": "response_item", "timestamp": "2026-01-02T04:04:05Z",
            "payload": {
                "type": "message", "id": id, "role": "assistant",
                "content": [{ "type": "output_text", "text": "hello" }]
            }
        })
    }

    fn tool_line(id: &str, kind: &str, name: &str) -> Value {
        json!({
            "type": "response_item", "timestamp": "2026-01-02T04:05:05Z",
            "payload": { "type": kind, "id": id, "name": name, "arguments": "{}" }
        })
    }

    fn usage_line(input: u64, output: u64, reasoning: u64, cached: u64) -> Value {
        json!({
            "type": "token_usage_record", "timestamp": "2026-01-02T04:04:05Z",
            "payload": {
                "usage": {
                    "input_tokens": input, "output_tokens": output,
                    "reasoning_output_tokens": reasoning, "cached_input_tokens": cached,
                    "cache_write_input_tokens": 0
                }
            }
        })
    }

    fn default_request() -> MiningRequest {
        MiningRequest::new(None, None, DEFAULT_TOP)
    }

    #[test]
    fn parses_sessions_messages_tools_and_tokens_without_cost() {
        let fixture = Fixture::new(
            "ses_1",
            &[
                session_meta(),
                json!({
                    "type": "turn_context", "timestamp": "2026-01-02T03:04:05Z",
                    "payload": { "cwd": "/home/work", "model": "gpt-5.6-terra" }
                }),
                user_line("m_1", "<environment_context>\ncwd\n</environment_context>"),
                user_line("m_2", "fix the bug"),
                assistant_line("m_3"),
                tool_line("t_1", "custom_tool_call", "exec"),
                tool_line("t_2", "function_call", "web_search"),
                usage_line(100, 40, 10, 1000),
            ],
        );

        let report = fixture.mine(&default_request());

        assert_eq!(report.totals.sessions, 1);
        assert_eq!(report.totals.messages_user, 2);
        assert_eq!(report.totals.messages_assistant, 1);
        assert_eq!(report.totals.tool_calls, 2);
        assert!(near(report.totals.cost_usd, 0.0));
        assert_eq!(report.totals.tokens_input, 100);
        assert_eq!(report.totals.tokens_output, 40);
        assert_eq!(report.totals.tokens_reasoning, 10);
        assert_eq!(report.totals.tokens_cache_read, 1000);

        assert_eq!(report.harnesses.len(), 1);
        assert_eq!(report.harnesses[0].label, "codex");
        assert!(near(report.harnesses[0].cost_usd, 0.0));
        assert_eq!(report.cost_by_harness[0].value, json!(0.0));
        assert_eq!(
            report.sessions_by_model,
            vec![crate::wire::responses::MiningSeriesPoint {
                label: "gpt-5.6-terra".into(),
                value: json!(1)
            }]
        );
        assert_eq!(report.tool_usage.len(), 2);
        assert_eq!(report.activity_by_day[0].label, "2026-01-02");
        assert_eq!(report.activity_by_day[0].values[3], 2);
        assert_eq!(report.activity_by_day[0].values[4], 1);

        let session = &report.sessions[0];
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.harness, "codex");
        assert_eq!(session.title, "fix the bug");
        assert_eq!(session.directory, "/home/work");
        assert_eq!(session.model.as_deref(), Some("gpt-5.6-terra"));
        assert!(near(session.cost_usd, 0.0));
    }

    #[test]
    fn a_directory_filter_matches_only_that_workspace() {
        let fixture = Fixture::new("ses_1", &[session_meta(), user_line("m_1", "hi")]);
        let request = MiningRequest::new(Some("/elsewhere".into()), None, DEFAULT_TOP);
        assert_eq!(
            fixture.mine(&request),
            crate::wire::responses::MiningReport::default()
        );
        let request = MiningRequest::new(Some("/home/work".into()), None, DEFAULT_TOP);
        assert_eq!(fixture.mine(&request).totals.sessions, 1);
    }

    #[test]
    fn parser_ignores_non_messages_and_invalid_message_shapes() {
        let request = default_request();
        assert!(SessionParser::parse(
            Path::new("missing.jsonl"),
            "not json\n[]\n{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\"}}",
            &request,
        )
        .is_none());

        let fixture = Fixture::new(
            "ses_1",
            &[
                session_meta(),
                json!({
                    "type": "response_item", "timestamp": "2026-01-02T03:04:05Z",
                    "payload": { "type": "message", "role": "tool" }
                }),
                json!({
                    "type": "response_item", "timestamp": "2026-01-02T04:04:05Z",
                    "payload": { "type": "message", "role": "user" }
                }),
                tool_line("t_1", "local_shell_call", "shell"),
                json!({ "type": "token_usage_record", "payload": { "usage": {} } }),
            ],
        );

        let report = fixture.mine(&request);

        assert_eq!(report.totals.messages_user, 1);
        assert_eq!(report.totals.tool_calls, 1);
        assert_eq!(report.tool_usage[0].label, "shell");
        assert_eq!(report.sessions[0].title, "");
    }

    #[test]
    fn a_since_filter_drops_sessions_created_before_the_window() {
        let fixture = Fixture::new(
            "ses_1",
            &[
                session_meta(),
                json!({
                    "type": "response_item", "timestamp": "2020-01-02T03:04:05Z",
                    "payload": { "type": "message", "role": "user", "content": [] }
                }),
            ],
        );
        let request = MiningRequest::new(None, Some(1), DEFAULT_TOP);

        assert_eq!(
            fixture.mine(&request),
            crate::wire::responses::MiningReport::default()
        );
    }

    #[test]
    fn an_empty_sessions_root_mines_nothing() {
        let root = std::env::temp_dir().join(format!("mitos-codex-{}", crate::domain::id()));
        std::fs::create_dir_all(&root).unwrap();
        let report = mine_codex(&root, &default_request());
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(report, crate::wire::responses::MiningReport::default());
    }
}
