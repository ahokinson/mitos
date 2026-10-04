use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use chrono::DateTime;
use libsql::params;

use crate::history::{ASSISTANT, ReadonlyDatabase, USER, empty_handoff, transcript_handoff};
use crate::json::{Record, count_value, number_value, parsed_container, string_value};
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{CollectedHandoff, CollectedMessage, CollectedUsage};

const DATABASE_NAMES: [&str; 2] = ["opencode-stable.db", "opencode.db"];
const RECENT_SESSIONS: i64 = 48;
const LAUNCH_SLACK_MS: f64 = 60_000.0;

/// `data_home` is the XDG data directory; the histories live under `opencode/`.
pub fn collect_opencode_handoff(data_home: &Path, request: &HandoffRequest) -> CollectedHandoff {
    let native_session = string_value(request.native_session.as_ref());
    for name in DATABASE_NAMES {
        let path = data_home.join("opencode").join(name);
        let Some(database) = ReadonlyDatabase::open(&path) else {
            continue;
        };
        // A database without the expected tables is not this harness's.
        if let Ok(Some(handoff)) = read_handoff(&database, request, native_session) {
            return handoff;
        }
    }
    empty_handoff(native_session.map(str::to_string))
}

fn read_handoff(
    database: &ReadonlyDatabase,
    request: &HandoffRequest,
    native_session: Option<&str>,
) -> Result<Option<CollectedHandoff>> {
    let Some(session) = find_session(database, request, native_session)? else {
        return Ok(None);
    };
    let Some(session_id) = string_value(session.get("id")).map(str::to_string) else {
        return Ok(None);
    };
    let messages = session_messages(database, &session_id)?;
    if messages.is_empty() {
        return Ok(Some(empty_handoff(Some(session_id))));
    }
    let usage = usage_of(&session);
    Ok(Some(transcript_handoff(session_id, messages, usage)))
}

#[allow(clippy::cast_precision_loss)]
fn find_session(
    database: &ReadonlyDatabase,
    request: &HandoffRequest,
    native_session: Option<&str>,
) -> Result<Option<Record>> {
    if let Some(session) = native_session {
        let rows = database.query("SELECT * FROM session WHERE id = ?", params![session])?;
        return Ok(rows.into_iter().next());
    }
    let Some(threshold) = DateTime::parse_from_rfc3339(&request.launched_at)
        .ok()
        .map(|launched| launched.timestamp_millis() as f64 - LAUNCH_SLACK_MS)
    else {
        return Ok(None);
    };
    let workdir = request.workdir.to_string_lossy().into_owned();
    let rows = database.query(
        "SELECT * FROM session WHERE directory = ? AND parent_id IS NULL ORDER BY time_updated DESC LIMIT ?",
        params![workdir, RECENT_SESSIONS],
    )?;
    Ok(rows
        .into_iter()
        .find(|row| number_value(row.get("time_updated")).unwrap_or(0.0) >= threshold))
}

fn session_messages(
    database: &ReadonlyDatabase,
    session_id: &str,
) -> Result<Vec<CollectedMessage>> {
    let messages = database.query(
        "SELECT id, data FROM message WHERE session_id = ? ORDER BY time_created, id",
        params![session_id],
    )?;
    let parts = database.query(
        "SELECT message_id, data FROM part WHERE session_id = ? ORDER BY time_created, id",
        params![session_id],
    )?;

    let mut text_by_message: HashMap<String, Vec<String>> = HashMap::new();
    for row in &parts {
        let Some(message_id) = string_value(row.get("message_id")) else {
            continue;
        };
        let Some(part) = row.get("data").and_then(parsed_container) else {
            continue;
        };
        let Some(part) = part.as_object() else {
            continue;
        };
        if part.get("type").and_then(|kind| kind.as_str()) != Some("text")
            || part.get("synthetic") == Some(&serde_json::Value::Bool(true))
            || part.get("ignored") == Some(&serde_json::Value::Bool(true))
        {
            continue;
        }
        if let Some(text) = string_value(part.get("text")) {
            text_by_message
                .entry(message_id.into())
                .or_default()
                .push(text.into());
        }
    }

    Ok(messages
        .iter()
        .filter_map(|row| {
            let id = string_value(row.get("id"))?;
            let data = row.get("data").and_then(parsed_container)?;
            let role = string_value(data.get("role"))?;
            if role != USER && role != ASSISTANT {
                return None;
            }
            let text = text_by_message.get(id)?.join("\n");
            let text = text.trim();
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
        count("tokens_input"),
        count("tokens_output"),
        count("tokens_cache_read"),
    );
    let cost = number_value(session.get("cost")).unwrap_or(0.0);
    let model = session
        .get("model")
        .and_then(parsed_container)
        .and_then(|model| string_value(model.get("id")).map(str::to_string));
    if input == 0 && output == 0 && cached == 0 && cost == 0.0 && model.is_none() {
        return None;
    }
    Some(CollectedUsage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: Some(cached),
        cost_usd: Some(cost),
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

    const SCHEMA: &str = "CREATE TABLE session (
        id TEXT PRIMARY KEY, directory TEXT NOT NULL, parent_id TEXT, model TEXT,
        tokens_input INTEGER DEFAULT 0 NOT NULL,
        tokens_output INTEGER DEFAULT 0 NOT NULL,
        tokens_cache_read INTEGER DEFAULT 0 NOT NULL,
        cost REAL DEFAULT 0 NOT NULL,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
        CREATE TABLE message (
        id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL,
        data TEXT NOT NULL);
        CREATE TABLE part (
        id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL,
        data TEXT NOT NULL);";

    struct Data {
        home: PathBuf,
        sql: String,
        clock: u32,
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn quote(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    impl Data {
        fn new() -> Self {
            let home = std::env::temp_dir().join(format!("mitos-opencode-{}", crate::domain::id()));
            std::fs::create_dir_all(home.join("opencode")).unwrap();
            Self {
                home,
                sql: SCHEMA.into(),
                clock: 1,
            }
        }

        /// `extra` are `column = SQL value` pairs overriding the defaults.
        fn session(&mut self, id: &str, directory: &str, extra: &[(&str, String)]) -> &mut Self {
            let now = now_ms().to_string();
            let mut columns: Vec<(&str, String)> = vec![
                ("id", quote(id)),
                ("directory", quote(directory)),
                ("time_created", now.clone()),
                ("time_updated", now),
            ];
            for (name, value) in extra {
                match columns.iter_mut().find(|(existing, _)| existing == name) {
                    Some(entry) => entry.1 = value.clone(),
                    None => columns.push((name, value.clone())),
                }
            }
            let names: Vec<_> = columns.iter().map(|(name, _)| *name).collect();
            let values: Vec<_> = columns.iter().map(|(_, value)| value.as_str()).collect();
            write!(
                self.sql,
                "INSERT INTO session ({}) VALUES ({});",
                names.join(", "),
                values.join(", ")
            )
            .unwrap();
            self
        }

        fn message(
            &mut self,
            session: &str,
            id: &str,
            role: &str,
            parts: &[serde_json::Value],
        ) -> &mut Self {
            self.clock += 1;
            let created = self.clock;
            write!(
                self.sql,
                "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES ({}, {}, {created}, {created}, {});",
                quote(id),
                quote(session),
                quote(&json!({ "role": role }).to_string())
            )
            .unwrap();
            for (index, part) in parts.iter().enumerate() {
                write!(
                    self.sql,
                    "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES ({}, {}, {}, {created}, {created}, {});",
                    quote(&format!("{id}_{index}")),
                    quote(id),
                    quote(session),
                    quote(&part.to_string())
                )
                .unwrap();
            }
            self
        }

        fn collect_from(&self, database: &str, native_session: Option<&str>) -> CollectedHandoff {
            create_database(&self.home.join("opencode").join(database), &self.sql);
            collect_opencode_handoff(&self.home, &request(native_session))
        }

        fn collect(&self, native_session: Option<&str>) -> CollectedHandoff {
            self.collect_from("opencode-stable.db", native_session)
        }
    }

    impl Drop for Data {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    fn request(native_session: Option<&str>) -> HandoffRequest {
        HandoffRequest::collect(
            "opencode",
            Path::new("/workspace"),
            chrono::Utc::now().to_rfc3339(),
            native_session.map(|session| json!(session)),
        )
    }

    fn texts(handoff: &CollectedHandoff) -> Vec<(String, String)> {
        handoff
            .transcript
            .iter()
            .flat_map(|transcript| &transcript.messages)
            .map(|message| (message.role.clone(), message.text.clone()))
            .collect()
    }

    fn text(value: &str) -> serde_json::Value {
        json!({ "type": "text", "text": value })
    }

    #[test]
    fn reads_transcript_and_usage_from_the_session_message_and_part_tables() {
        let mut data = Data::new();
        data.session(
            "ses_1",
            "/workspace",
            &[
                (
                    "model",
                    quote(&json!({ "id": "glm-5.3", "providerID": "opencode-go" }).to_string()),
                ),
                ("tokens_input", "250".into()),
                ("tokens_output", "90".into()),
                ("tokens_cache_read", "1000".into()),
                ("cost", "2.5".into()),
            ],
        )
        .message("ses_1", "msg_1", "user", &[text("hi")])
        .message(
            "ses_1",
            "msg_2",
            "assistant",
            &[
                json!({ "type": "step-start" }),
                json!({ "type": "reasoning", "text": "thinking" }),
                text("hello"),
                json!({ "type": "tool", "tool": "bash" }),
                text("all done"),
            ],
        );

        let handoff = data.collect(None);

        assert_eq!(handoff.native_session, Some(json!("ses_1")));
        assert_eq!(
            texts(&handoff),
            [
                ("user".into(), "hi".into()),
                ("assistant".into(), "hello\nall done".into())
            ]
        );
        let usage = handoff.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(250));
        assert_eq!(usage.output_tokens, Some(90));
        assert_eq!(usage.cached_input_tokens, Some(1000));
        assert_eq!(usage.cost_usd, Some(2.5));
        assert_eq!(usage.model.as_deref(), Some("glm-5.3"));
        assert_eq!(usage.turns, Some(1));
    }

    #[test]
    fn a_requested_native_session_wins_over_a_newer_one_in_the_workdir() {
        let mut data = Data::new();
        data.session("ses_old", "/elsewhere", &[])
            .session("ses_new", "/workspace", &[])
            .message("ses_old", "msg_1", "assistant", &[text("old")])
            .message("ses_new", "msg_2", "assistant", &[text("new")]);

        let handoff = data.collect(Some("ses_old"));

        assert_eq!(handoff.native_session, Some(json!("ses_old")));
        assert_eq!(texts(&handoff), [("assistant".into(), "old".into())]);
    }

    #[test]
    fn synthetic_and_ignored_text_parts_are_left_out() {
        let mut data = Data::new();
        data.session("ses_1", "/workspace", &[]).message(
            "ses_1",
            "msg_1",
            "user",
            &[
                text("real"),
                json!({ "type": "text", "text": "injected", "synthetic": true }),
                json!({ "type": "text", "text": "hidden", "ignored": true }),
            ],
        );
        assert_eq!(texts(&data.collect(None)), [("user".into(), "real".into())]);
    }

    #[test]
    fn a_subagent_session_is_not_picked_for_the_workdir() {
        let mut data = Data::new();
        data.session(
            "ses_child",
            "/workspace",
            &[("parent_id", quote("ses_parent"))],
        )
        .message("ses_child", "msg_1", "assistant", &[text("child")]);
        assert!(data.collect(None).transcript.is_none());
    }

    #[test]
    fn a_stale_session_in_the_workdir_is_not_picked() {
        let stale = (now_ms() - 3_600_000).to_string();
        let mut data = Data::new();
        data.session("ses_1", "/workspace", &[("time_updated", stale)])
            .message("ses_1", "msg_1", "assistant", &[text("old")]);
        assert!(data.collect(None).transcript.is_none());
    }

    #[test]
    fn a_found_session_without_token_data_still_reports_its_turn_count() {
        let mut data = Data::new();
        data.session("ses_1", "/workspace", &[]).message(
            "ses_1",
            "msg_1",
            "assistant",
            &[text("hello")],
        );
        let usage = data.collect(None).usage.unwrap();
        assert_eq!(usage.turns, Some(1));
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.cost_usd, None);
    }

    #[test]
    fn falls_back_to_opencode_db_when_the_stable_database_is_absent() {
        let mut data = Data::new();
        data.session("ses_1", "/workspace", &[])
            .message("ses_1", "msg_1", "user", &[text("hi")]);
        let handoff = data.collect_from("opencode.db", None);
        assert_eq!(handoff.native_session, Some(json!("ses_1")));
    }

    #[test]
    fn usage_and_transcript_are_absent_when_nothing_matches() {
        let mut data = Data::new();
        data.session("ses_1", "/somewhere-else", &[]);
        let handoff = data.collect(None);
        assert!(handoff.transcript.is_none());
        assert!(handoff.usage.is_none());
    }

    #[test]
    fn no_database_at_all_yields_an_empty_handoff() {
        let data = Data::new();
        let handoff = collect_opencode_handoff(&data.home, &request(Some("ses_x")));
        assert_eq!(handoff.native_session, Some(json!("ses_x")));
        assert!(handoff.transcript.is_none());
    }
}
