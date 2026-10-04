use std::path::Path;

use anyhow::Result;
use chrono::DateTime;
use libsql::params;

use crate::history::{ASSISTANT, ReadonlyDatabase, USER, empty_handoff, transcript_handoff};
use crate::json::{Record, count_value, number_value, string_value};
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{CollectedHandoff, CollectedMessage, CollectedUsage};

const RECENT_SESSIONS: i64 = 24;
const LAUNCH_SLACK_MS: f64 = 60_000.0;

/// An unreadable or missing history is an empty handoff, not a failure.
pub fn collect_hermes_handoff(home: &Path, request: &HandoffRequest) -> CollectedHandoff {
    let requested = string_value(request.native_session.as_ref()).map(str::to_string);
    let Some(database) = ReadonlyDatabase::open(&home.join("state.db")) else {
        return empty_handoff(requested);
    };
    read_handoff(&database, request, requested.as_deref())
        .unwrap_or_else(|_| empty_handoff(requested))
}

fn read_handoff(
    database: &ReadonlyDatabase,
    request: &HandoffRequest,
    requested: Option<&str>,
) -> Result<CollectedHandoff> {
    let session = find_session(database, request, requested)?;
    let session_id = session
        .as_ref()
        .and_then(|session| string_value(session.get("id")))
        .map(str::to_string);
    let (Some(session), Some(session_id)) = (session, session_id) else {
        return Ok(empty_handoff(requested.map(str::to_string)));
    };
    let messages = session_messages(database, &session_id)?;
    if messages.is_empty() {
        return Ok(empty_handoff(Some(session_id)));
    }
    let usage = usage_of(&session);
    Ok(transcript_handoff(session_id, messages, usage))
}

#[allow(clippy::cast_precision_loss)]
fn find_session(
    database: &ReadonlyDatabase,
    request: &HandoffRequest,
    requested: Option<&str>,
) -> Result<Option<Record>> {
    if let Some(requested) = requested {
        let rows = database.query("SELECT * FROM sessions WHERE id = ?", params![requested])?;
        return Ok(rows.into_iter().next());
    }
    let threshold = DateTime::parse_from_rfc3339(&request.launched_at)
        .ok()
        .map(|launched| launched.timestamp_millis() as f64 - LAUNCH_SLACK_MS);
    let Some(threshold) = threshold else {
        return Ok(None);
    };
    let workdir = request.workdir.to_string_lossy().into_owned();
    let rows = database.query(
        "SELECT * FROM sessions WHERE cwd = ? AND archived = 0 AND hidden = 0 ORDER BY started_at DESC LIMIT ?",
        params![workdir, RECENT_SESSIONS],
    )?;
    Ok(rows.into_iter().find(|row| touched_at_ms(row) >= threshold))
}

/// Timestamps are epoch seconds.
fn touched_at_ms(session: &Record) -> f64 {
    let seconds = ["last_activity_at", "ended_at", "started_at"]
        .iter()
        .find_map(|key| number_value(session.get(*key)))
        .unwrap_or(0.0);
    seconds * 1000.0
}

fn session_messages(
    database: &ReadonlyDatabase,
    session_id: &str,
) -> Result<Vec<CollectedMessage>> {
    let rows = database.query(
        "SELECT role, content FROM messages WHERE session_id = ? ORDER BY id",
        params![session_id],
    )?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let role =
                string_value(row.get("role")).filter(|role| *role == USER || *role == ASSISTANT)?;
            let text = string_value(row.get("content"))?.trim();
            (!text.is_empty()).then(|| CollectedMessage {
                role: role.into(),
                text: text.into(),
            })
        })
        .collect())
}

fn usage_of(session: &Record) -> Option<CollectedUsage> {
    let count = |key: &str| count_value(session.get(key)).unwrap_or(0);
    let (input, output, cached) = (
        count("input_tokens"),
        count("output_tokens"),
        count("cache_read_tokens"),
    );
    let cost = number_value(session.get("actual_cost_usd"))
        .or_else(|| number_value(session.get("estimated_cost_usd")));
    let model = string_value(session.get("model")).map(str::to_string);
    if input == 0 && output == 0 && cached == 0 && cost.is_none() && model.is_none() {
        return None;
    }
    Some(CollectedUsage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: Some(cached),
        cost_usd: cost,
        model,
        ..CollectedUsage::default()
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::history::create_database;

    const SCHEMA: &str = "CREATE TABLE sessions (
        id TEXT PRIMARY KEY, source TEXT NOT NULL, model TEXT,
        started_at REAL NOT NULL, ended_at REAL, last_activity_at REAL,
        input_tokens INTEGER DEFAULT 0, output_tokens INTEGER DEFAULT 0,
        cache_read_tokens INTEGER DEFAULT 0, cwd TEXT,
        estimated_cost_usd REAL, actual_cost_usd REAL,
        archived INTEGER NOT NULL DEFAULT 0, hidden INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL,
        role TEXT NOT NULL, content TEXT, tool_name TEXT, timestamp REAL NOT NULL);";

    struct Home {
        dir: PathBuf,
        sql: String,
    }

    #[allow(clippy::cast_precision_loss)]
    fn now_seconds() -> f64 {
        chrono::Utc::now().timestamp_millis() as f64 / 1000.0
    }

    impl Home {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("mitos-hermes-{}", crate::domain::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                dir,
                sql: SCHEMA.into(),
            }
        }

        /// `columns` are extra `column = value` SQL fragments for the session row.
        fn session(&mut self, id: &str, cwd: &str, extra: &[(&str, &str)]) -> &mut Self {
            let mut columns = vec![
                ("id", format!("'{id}'")),
                ("source", "'cli'".into()),
                ("cwd", format!("'{cwd}'")),
            ];
            if !extra.iter().any(|(name, _)| *name == "started_at") {
                columns.push(("started_at", now_seconds().to_string()));
                if !extra.iter().any(|(name, _)| *name == "last_activity_at") {
                    columns.push(("last_activity_at", now_seconds().to_string()));
                }
            }
            columns.extend(extra.iter().map(|(name, value)| (*name, value.to_string())));
            let names: Vec<_> = columns.iter().map(|(name, _)| *name).collect();
            let values: Vec<_> = columns.iter().map(|(_, value)| value.as_str()).collect();
            write!(
                self.sql,
                "INSERT INTO sessions ({}) VALUES ({});",
                names.join(", "),
                values.join(", ")
            )
            .unwrap();
            self
        }

        fn message(&mut self, session: &str, role: &str, content: &str) -> &mut Self {
            write!(
                self.sql,
                "INSERT INTO messages (session_id, role, content, timestamp) VALUES ('{session}', '{role}', '{content}', {});",
                now_seconds()
            )
            .unwrap();
            self
        }

        fn collect(&self, native_session: Option<&str>) -> CollectedHandoff {
            create_database(&self.dir.join("state.db"), &self.sql);
            collect_hermes_handoff(&self.dir, &request(native_session))
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn request(native_session: Option<&str>) -> HandoffRequest {
        HandoffRequest::collect(
            "hermes",
            Path::new("/workspace"),
            chrono::Utc::now().to_rfc3339(),
            native_session.map(|session| json!(session)),
        )
    }

    fn texts(handoff: &CollectedHandoff) -> Vec<(String, String)> {
        handoff
            .transcript
            .as_ref()
            .map(|transcript| {
                transcript
                    .messages
                    .iter()
                    .map(|message| (message.role.clone(), message.text.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn reads_transcript_and_usage_from_the_sessions_and_messages_tables() {
        let mut home = Home::new();
        home.session(
            "20260829_230910_a6f34e",
            "/workspace",
            &[
                ("model", "'deepseek/deepseek-v4-flash'"),
                ("input_tokens", "17372"),
                ("output_tokens", "101"),
                ("cache_read_tokens", "2048"),
                ("estimated_cost_usd", "0.25"),
            ],
        )
        .message("20260829_230910_a6f34e", "user", "hi")
        .message("20260829_230910_a6f34e", "tool", "ls output")
        .message("20260829_230910_a6f34e", "assistant", "hello");

        let handoff = home.collect(None);

        assert_eq!(
            handoff.native_session,
            Some(json!("20260829_230910_a6f34e"))
        );
        assert_eq!(
            texts(&handoff),
            [
                ("user".into(), "hi".into()),
                ("assistant".into(), "hello".into())
            ]
        );
        let usage = handoff.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(17_372));
        assert_eq!(usage.output_tokens, Some(101));
        assert_eq!(usage.cached_input_tokens, Some(2048));
        assert_eq!(usage.cost_usd, Some(0.25));
        assert_eq!(usage.model.as_deref(), Some("deepseek/deepseek-v4-flash"));
        assert_eq!(usage.turns, Some(1));
    }

    #[test]
    fn actual_cost_wins_over_the_estimate_and_no_cost_leaves_it_unset() {
        let mut priced = Home::new();
        priced
            .session(
                "s1",
                "/workspace",
                &[
                    ("input_tokens", "1"),
                    ("output_tokens", "1"),
                    ("estimated_cost_usd", "0.5"),
                    ("actual_cost_usd", "0.4"),
                ],
            )
            .message("s1", "assistant", "hello");
        assert_eq!(priced.collect(None).usage.unwrap().cost_usd, Some(0.4));

        let mut free = Home::new();
        free.session(
            "s1",
            "/workspace",
            &[("input_tokens", "1"), ("output_tokens", "1")],
        )
        .message("s1", "assistant", "hello");
        assert_eq!(free.collect(None).usage.unwrap().cost_usd, None);
    }

    #[test]
    fn a_requested_native_session_wins_over_another_in_the_workdir() {
        let mut home = Home::new();
        home.session("old", "/elsewhere", &[])
            .session("new", "/workspace", &[])
            .message("old", "assistant", "old reply")
            .message("new", "assistant", "new reply");

        let handoff = home.collect(Some("old"));

        assert_eq!(handoff.native_session, Some(json!("old")));
        assert_eq!(texts(&handoff), [("assistant".into(), "old reply".into())]);
    }

    #[test]
    fn falls_back_to_start_time_when_a_session_has_no_recorded_activity() {
        let mut home = Home::new();
        home.session("s1", "/workspace", &[("last_activity_at", "NULL")])
            .message("s1", "assistant", "hello");
        assert_eq!(home.collect(None).native_session, Some(json!("s1")));
    }

    #[test]
    fn a_stale_session_in_the_workdir_is_not_picked() {
        let hour_ago = (now_seconds() - 3600.0).to_string();
        let mut home = Home::new();
        home.session(
            "s1",
            "/workspace",
            &[("started_at", &hour_ago), ("last_activity_at", &hour_ago)],
        )
        .message("s1", "assistant", "old");
        assert!(home.collect(None).transcript.is_none());
    }

    #[test]
    fn archived_and_hidden_sessions_are_not_picked() {
        let mut home = Home::new();
        home.session("a", "/workspace", &[("archived", "1")])
            .session("h", "/workspace", &[("hidden", "1")])
            .message("a", "assistant", "archived")
            .message("h", "assistant", "hidden");
        assert!(home.collect(None).transcript.is_none());
    }

    #[test]
    fn a_found_session_without_token_data_still_reports_its_turn_count() {
        let mut home = Home::new();
        home.session("s1", "/workspace", &[])
            .message("s1", "assistant", "hello");
        let usage = home.collect(None).usage.unwrap();
        assert_eq!(usage.turns, Some(1));
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn usage_and_transcript_are_absent_when_nothing_matches() {
        let mut home = Home::new();
        home.session("s1", "/somewhere-else", &[]);
        let handoff = home.collect(None);
        assert!(handoff.transcript.is_none());
        assert!(handoff.usage.is_none());
    }

    #[test]
    fn no_database_at_all_yields_an_empty_handoff() {
        let home = Home::new();
        let handoff = collect_hermes_handoff(&home.dir, &request(Some("s9")));
        assert_eq!(handoff.native_session, Some(json!("s9")));
        assert!(handoff.transcript.is_none());
    }
}
