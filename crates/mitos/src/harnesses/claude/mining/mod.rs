mod parser;

use std::path::{Path, PathBuf};

use crate::domain::HarnessKind;
use crate::harnesses::mining::ReportBuilder;
use crate::wire::requests::MiningRequest;

use parser::SessionParser;

/// `projects_root` is `~/.claude/projects`; one `<uuid>.jsonl` directly
/// under each project directory is one session. Subagent sidechains live in
/// `<uuid>/subagents/` and their cost is already inside the parent's totals,
/// so they are not walked.
pub fn mine_claude(
    projects_root: &Path,
    request: &MiningRequest,
) -> crate::wire::responses::MiningReport {
    let mut builder = ReportBuilder::new(HarnessKind::Claude.as_str());
    for path in session_files(projects_root) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if let Some(stats) = SessionParser::parse(&path, &text, request) {
            builder.absorb(stats);
        }
    }
    builder.finish().attributed(HarnessKind::Claude.as_str())
}

fn session_files(projects_root: &Path) -> Vec<PathBuf> {
    let Ok(projects) = std::fs::read_dir(projects_root) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            if is_file && path.extension().is_some_and(|ext| ext == "jsonl") {
                files.push(path);
            }
        }
    }
    files
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
        /// Writes one session file under one project directory.
        fn new(id: &str, lines: &[Value]) -> Self {
            let root = std::env::temp_dir().join(format!("mitos-claude-{}", crate::domain::id()));
            let project = root.join("-home-work");
            std::fs::create_dir_all(&project).unwrap();
            let body: Vec<String> = lines.iter().map(Value::to_string).collect();
            std::fs::write(project.join(format!("{id}.jsonl")), body.join("\n")).unwrap();
            Self { root }
        }

        fn mine(&self, request: &MiningRequest) -> crate::wire::responses::MiningReport {
            mine_claude(&self.root, request)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn user_line(text: &str) -> Value {
        json!({
            "type": "user", "cwd": "/home/work", "timestamp": "2026-01-02T03:04:05Z",
            "message": { "role": "user", "content": [{ "type": "text", "text": text }] }
        })
    }

    fn assistant_line(id: &str) -> Value {
        json!({
            "type": "assistant", "cwd": "/home/work", "timestamp": "2026-01-02T04:04:05Z",
            "message": {
                "id": id, "role": "assistant", "model": "claude-sonnet-5",
                "content": [
                    { "type": "text", "text": "hello" },
                    { "type": "tool_use", "name": "Bash", "input": {} }
                ],
                "usage": {
                    "input_tokens": 100, "output_tokens": 40,
                    "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 200,
                    "output_tokens_details": { "thinking_tokens": 10 }
                }
            }
        })
    }

    fn default_request() -> MiningRequest {
        MiningRequest::new(None, None, DEFAULT_TOP)
    }

    #[test]
    fn parses_sessions_totals_costs_models_and_tools() {
        let fixture = Fixture::new(
            "ses_1",
            &[
                user_line("fix the bug"),
                assistant_line("msg_1"),
                json!({
                    "type": "cost-state", "sessionId": "ses_1",
                    "totalCostUSD": 2.5, "totalToolDuration": 8000,
                    "modelUsage": { "claude-sonnet-5": { "costUSD": 2.5 } }
                }),
                json!({ "type": "ai-title", "aiTitle": "Fix the bug" }),
                json!({ "type": "agent-name", "agentName": "build" }),
            ],
        );

        let report = fixture.mine(&default_request());

        assert_eq!(report.totals.sessions, 1);
        assert_eq!(report.totals.messages_user, 1);
        assert_eq!(report.totals.messages_assistant, 1);
        assert_eq!(report.totals.tool_calls, 1);
        assert!(near(report.totals.cost_usd, 2.5));
        assert!(near(report.totals.tool_seconds, 8.0));
        assert_eq!(report.totals.tokens_input, 100);
        assert_eq!(report.totals.tokens_output, 40);
        assert_eq!(report.totals.tokens_reasoning, 10);
        assert_eq!(report.totals.tokens_cache_read, 1000);
        assert_eq!(report.totals.tokens_cache_write, 200);

        assert_eq!(report.harnesses.len(), 1);
        assert_eq!(report.harnesses[0].label, "claude");
        assert_eq!(report.cost_by_harness[0].value, json!(2.5));
        assert_eq!(
            report.cost_by_model,
            vec![crate::wire::responses::MiningSeriesPoint {
                label: "claude-sonnet-5".into(),
                value: json!(2.5)
            }]
        );
        assert_eq!(
            report.sessions_by_agent,
            vec![crate::wire::responses::MiningSeriesPoint {
                label: "build".into(),
                value: json!(1)
            }]
        );
        assert_eq!(report.tool_usage[0].label, "Bash");
        assert_eq!(report.activity_by_day[0].label, "2026-01-02");
        assert_eq!(report.activity_by_day[0].values[4], 1);

        let session = &report.sessions[0];
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.harness, "claude");
        assert_eq!(session.title, "Fix the bug");
        assert_eq!(session.directory, "/home/work");
        assert_eq!(session.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(session.agent.as_deref(), Some("build"));
    }

    #[test]
    fn a_user_line_of_tool_results_is_not_a_message() {
        let fixture = Fixture::new(
            "ses_1",
            &[
                user_line("hi"),
                json!({
                    "type": "user", "timestamp": "2026-01-02T05:04:05Z",
                    "message": { "role": "user", "content": [
                        { "type": "tool_result", "content": "output" }
                    ] }
                }),
            ],
        );

        let report = fixture.mine(&default_request());

        assert_eq!(report.totals.messages_user, 1);
        assert_eq!(report.totals.messages_assistant, 0);
    }

    #[test]
    fn a_directory_filter_matches_only_that_workspace() {
        let fixture = Fixture::new("ses_1", &[user_line("hi")]);
        let request = MiningRequest::new(Some("/elsewhere".into()), None, DEFAULT_TOP);
        assert_eq!(
            fixture.mine(&request),
            crate::wire::responses::MiningReport::default()
        );
        let request = MiningRequest::new(Some("/home/work".into()), None, DEFAULT_TOP);
        assert_eq!(fixture.mine(&request).totals.sessions, 1);
    }

    #[test]
    fn a_since_filter_drops_sessions_created_before_the_window() {
        let old = chrono::Utc::now() - chrono::Duration::days(40);
        let fixture = Fixture::new(
            "ses_1",
            &[json!({
                "type": "user", "cwd": "/home/work",
                "timestamp": old.to_rfc3339(),
                "message": { "role": "user", "content": "hi" }
            })],
        );
        let request = MiningRequest::new(None, Some(30), DEFAULT_TOP);
        assert_eq!(
            fixture.mine(&request),
            crate::wire::responses::MiningReport::default()
        );
    }

    #[test]
    fn an_empty_project_root_mines_nothing() {
        let root = std::env::temp_dir().join(format!("mitos-claude-{}", crate::domain::id()));
        std::fs::create_dir_all(&root).unwrap();
        let report = mine_claude(&root, &default_request());
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(report, crate::wire::responses::MiningReport::default());
    }
}
