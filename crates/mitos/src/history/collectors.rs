use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::files::{Candidate, discover_files};
use super::handoffs::empty_handoff;
use super::messages::{matches_workspace, messages_from_record};
use super::usages::usage_from_records;
use crate::json::{find_string, string_value};
use crate::wire::requests::HandoffRequest;
use crate::wire::responses::{CollectedHandoff, CollectedTranscript};

const MAX_FILES: usize = 24;
const LAUNCH_SLACK_SECONDS: i64 = 60;

pub struct JsonlSearch<'a> {
    pub root: &'a Path,
    pub workdir: &'a Path,
    pub launched_at: Option<DateTime<Utc>>,
    pub native_session: Option<&'a Value>,
}

pub fn collect_requested_handoff(
    root: &Path,
    request: &HandoffRequest,
) -> Result<CollectedHandoff> {
    let launched_at = DateTime::parse_from_rfc3339(request.launched_at.as_str())
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc));
    collect_jsonl_handoff(&JsonlSearch {
        root,
        workdir: &request.workdir,
        launched_at,
        native_session: request.native_session.as_ref(),
    })
}

pub fn collect_jsonl_handoff(search: &JsonlSearch<'_>) -> Result<CollectedHandoff> {
    let native_session = string_value(search.native_session);
    let workdir = search.workdir.to_string_lossy();
    for candidate in find_candidates(search, native_session) {
        let bytes = std::fs::read(&candidate.path)
            .with_context(|| format!("could not read {}", candidate.path.display()))?;
        let records = parse_json_lines(&String::from_utf8_lossy(&bytes));
        if native_session.is_none() && !matches_workspace(&records, &workdir) {
            continue;
        }
        let messages: Vec<_> = records.iter().flat_map(messages_from_record).collect();
        if messages.is_empty() {
            continue;
        }
        let session = native_session
            .map(str::to_string)
            .or_else(|| session_from_records(&records))
            .or_else(|| session_from_path(&candidate.path));
        let usage = usage_from_records(&records, &messages);
        return Ok(CollectedHandoff::new(
            session.map(Value::String),
            Some(CollectedTranscript { messages }),
            usage,
        ));
    }
    Ok(empty_handoff(native_session.map(str::to_string)))
}

fn find_candidates(search: &JsonlSearch<'_>, native_session: Option<&str>) -> Vec<Candidate> {
    let threshold = search.launched_at.map(|launched| {
        std::time::SystemTime::from(launched - Duration::seconds(LAUNCH_SLACK_SECONDS))
    });
    let mut candidates: Vec<Candidate> = discover_files(search.root, ".jsonl", MAX_FILES * 8)
        .into_iter()
        .filter(|candidate| match native_session {
            Some(session) => candidate.path.to_string_lossy().contains(session),
            None => threshold.is_some_and(|threshold| candidate.modified >= threshold),
        })
        .collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    candidates.truncate(MAX_FILES);
    candidates
}

fn parse_json_lines(text: &str) -> Vec<Value> {
    text.split('\n')
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn session_from_records(records: &[Value]) -> Option<String> {
    records
        .iter()
        .find_map(|record| find_string(record, &["thread_id", "session_id", "sessionId", "id"]))
}

fn session_from_path(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_string_lossy();
    name.as_bytes()
        .windows(UUID_LEN)
        .find(|window| is_uuid(window))
        .map(|window| String::from_utf8_lossy(window).into_owned())
}

const UUID_LEN: usize = 36;
const UUID_DASHES: [usize; 4] = [8, 13, 18, 23];

fn is_uuid(window: &[u8]) -> bool {
    window.iter().enumerate().all(|(index, byte)| {
        if UUID_DASHES.contains(&index) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    const SESSION: &str = "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0";

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(lines: &[Value]) -> Self {
            let root = std::env::temp_dir().join(format!("mitos-jsonl-{}", crate::domain::id()));
            std::fs::create_dir_all(root.join("proj")).unwrap();
            let body: Vec<String> = lines.iter().map(Value::to_string).collect();
            std::fs::write(
                root.join("proj").join(format!("{SESSION}.jsonl")),
                body.join("\n"),
            )
            .unwrap();
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn lines() -> Vec<Value> {
        vec![
            json!({ "cwd": "/work", "type": "user", "message": { "role": "user", "content": "hi" } }),
            json!({ "type": "assistant", "message": { "id": "m1", "role": "assistant", "content": [{ "type": "text", "text": "hello" }], "usage": { "input_tokens": 3, "output_tokens": 2 } } }),
        ]
    }

    fn search<'a>(
        fixture: &'a Fixture,
        workdir: &'a Path,
        session: Option<&'a Value>,
    ) -> JsonlSearch<'a> {
        JsonlSearch {
            root: &fixture.root,
            workdir,
            launched_at: Some(Utc::now() - Duration::seconds(10)),
            native_session: session,
        }
    }

    #[test]
    fn the_session_is_read_from_the_path_when_no_record_names_one() {
        assert_eq!(
            session_from_path(Path::new(&format!("/p/{SESSION}.jsonl"))).as_deref(),
            Some(SESSION)
        );
        assert_eq!(session_from_path(Path::new("/p/notes.jsonl")), None);
    }

    #[test]
    fn finds_a_transcript_by_workspace_and_takes_the_first_id_a_record_carries() {
        let fixture = Fixture::new(&lines());
        let handoff = collect_jsonl_handoff(&search(&fixture, Path::new("/work"), None)).unwrap();
        assert_eq!(handoff.native_session, Some(json!("m1")));
        let transcript = handoff.transcript.unwrap();
        assert_eq!(transcript.messages.len(), 2);
        let usage = handoff.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(3));
        assert_eq!(usage.turns, Some(1));
    }

    #[test]
    fn a_different_workspace_yields_an_empty_handoff() {
        let fixture = Fixture::new(&lines());
        let handoff =
            collect_jsonl_handoff(&search(&fixture, Path::new("/elsewhere"), None)).unwrap();
        assert!(handoff.transcript.is_none());
        assert!(handoff.native_session.is_none());
    }

    #[test]
    fn a_known_session_matches_by_path_regardless_of_workspace_or_age() {
        let fixture = Fixture::new(&lines());
        let session = json!(SESSION);
        let mut request = search(&fixture, Path::new("/elsewhere"), Some(&session));
        request.launched_at = Some(Utc::now() + Duration::days(1));
        let handoff = collect_jsonl_handoff(&request).unwrap();
        assert_eq!(handoff.transcript.unwrap().messages.len(), 2);
    }
}
