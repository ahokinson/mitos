use std::collections::BTreeMap;

use libsql::Value as SqlValue;
use libsql::params_from_iter;
use serde_json::Value;

use super::{JOINED_FILTER, SESSION_FILTER};
use crate::domain::HarnessKind;
use crate::history::ReadonlyDatabase;
use crate::json::{count_value, number_value, string_value};
use crate::wire::requests::MiningRequest;
use crate::wire::responses::{MinedSession, MiningActivityDay, MiningSeriesPoint, MiningTotals};

const HOURS_PER_DAY: usize = 24;
/// The provider ceiling; the merged report is trimmed to the request's `top`.
const SESSIONS_CEILING: i64 = 1000;

/// Cost and session counts grouped by the session's agent, rank by value.
pub(super) fn agent_series(
    database: &ReadonlyDatabase,
    request: &MiningRequest,
) -> (Vec<MiningSeriesPoint>, Vec<MiningSeriesPoint>) {
    (
        series(
            database,
            &format!(
                "SELECT agent AS label, ROUND(SUM(cost), 4) AS value \
                 FROM session WHERE {SESSION_FILTER} AND agent IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
        series(
            database,
            &format!(
                "SELECT agent AS label, COUNT(*) AS value \
                 FROM session WHERE {SESSION_FILTER} AND agent IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
    )
}

/// Call counts and summed seconds grouped by the tool's own name, rank by
/// value; a tool call without a finished `state.time` counts but times nothing.
pub(super) fn tool_series(
    database: &ReadonlyDatabase,
    request: &MiningRequest,
) -> (Vec<MiningSeriesPoint>, Vec<MiningSeriesPoint>) {
    (
        series(
            database,
            &format!(
                "SELECT json_extract(p.data, '$.tool') AS label, COUNT(*) AS value \
                 FROM part p JOIN session s ON p.session_id = s.id \
                 WHERE {JOINED_FILTER} \
                 AND json_extract(p.data, '$.type') = 'tool' \
                 AND json_extract(p.data, '$.tool') IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
        series(
            database,
            &format!(
                "SELECT json_extract(p.data, '$.tool') AS label, \
                 ROUND(SUM((CAST(json_extract(p.data, '$.state.time.end') AS INTEGER) \
                     - CAST(json_extract(p.data, '$.state.time.start') AS INTEGER)) / 1000.0), 1) \
                     AS value \
                 FROM part p JOIN session s ON p.session_id = s.id \
                 WHERE {JOINED_FILTER} \
                 AND json_extract(p.data, '$.type') = 'tool' \
                 AND json_extract(p.data, '$.state.time.end') IS NOT NULL \
                 AND json_extract(p.data, '$.tool') IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
    )
}

fn binds(request: &MiningRequest) -> Vec<SqlValue> {
    let text =
        |value: Option<&str>| value.map_or(SqlValue::Null, |s| SqlValue::Text(s.to_string()));
    vec![
        text(request.directory.as_deref()),
        text(request.directory.as_deref()),
        request.since_ms.map_or(SqlValue::Null, SqlValue::Integer),
        request.since_ms.map_or(SqlValue::Null, SqlValue::Integer),
    ]
}

pub(super) fn totals(
    database: &ReadonlyDatabase,
    request: &MiningRequest,
) -> anyhow::Result<MiningTotals> {
    let rows = database.query(
        &format!(
            "SELECT COUNT(*) AS sessions, \
             COALESCE(ROUND(SUM(cost), 4), 0) AS cost_usd, \
             COALESCE(SUM(tokens_input), 0) AS tokens_input, \
             COALESCE(SUM(tokens_output), 0) AS tokens_output, \
             COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
             COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
             COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
             COALESCE(SUM(CASE WHEN parent_id IS NOT NULL THEN 1 ELSE 0 END), 0) AS forks \
             FROM session WHERE {SESSION_FILTER}"
        ),
        params_from_iter(binds(request)),
    )?;
    let session_row = rows.first().cloned().unwrap_or_default();
    let message_rows = database.query(
        &format!(
            "SELECT COALESCE(SUM(CASE WHEN json_extract(m.data, '$.role') = 'user' \
              THEN 1 ELSE 0 END), 0) AS messages_user, \
              COALESCE(SUM(CASE WHEN json_extract(m.data, '$.role') = 'assistant' \
              THEN 1 ELSE 0 END), 0) AS messages_assistant \
              FROM message m JOIN session s ON m.session_id = s.id WHERE {JOINED_FILTER}"
        ),
        params_from_iter(binds(request)),
    )?;
    let message_row = message_rows.first().cloned().unwrap_or_default();
    let part_rows = database.query(
        &format!(
            "SELECT COALESCE(SUM(CASE WHEN json_extract(p.data, '$.type') = 'tool' \
              THEN 1 ELSE 0 END), 0) AS tool_calls, \
              COALESCE(SUM(CASE WHEN json_extract(p.data, '$.type') = 'patch' \
              THEN 1 ELSE 0 END), 0) AS patches, \
              COALESCE(SUM(CASE WHEN json_extract(p.data, '$.type') = 'patch' \
              THEN json_array_length(json_extract(p.data, '$.files')) ELSE 0 END), 0) \
              AS files_patched, \
              COALESCE(ROUND(SUM(CASE WHEN json_extract(p.data, '$.type') = 'tool' \
              AND json_extract(p.data, '$.state.time.end') IS NOT NULL \
              THEN (CAST(json_extract(p.data, '$.state.time.end') AS INTEGER) \
              - CAST(json_extract(p.data, '$.state.time.start') AS INTEGER)) / 1000.0 \
              ELSE 0 END), 1), 0) AS tool_seconds \
              FROM part p JOIN session s ON p.session_id = s.id WHERE {JOINED_FILTER}"
        ),
        params_from_iter(binds(request)),
    )?;
    let part_row = part_rows.first().cloned().unwrap_or_default();
    let count = |row: &crate::json::Record, key: &str| count_value(row.get(key)).unwrap_or(0);
    Ok(MiningTotals {
        sessions: count(&session_row, "sessions"),
        messages: count(&message_row, "messages_user") + count(&message_row, "messages_assistant"),
        messages_user: count(&message_row, "messages_user"),
        messages_assistant: count(&message_row, "messages_assistant"),
        tool_calls: count(&part_row, "tool_calls"),
        patches: count(&part_row, "patches"),
        files_patched: count(&part_row, "files_patched"),
        forks: count(&session_row, "forks"),
        cost_usd: number_value(session_row.get("cost_usd")).unwrap_or(0.0),
        tool_seconds: number_value(part_row.get("tool_seconds")).unwrap_or(0.0),
        tokens_input: count(&session_row, "tokens_input"),
        tokens_output: count(&session_row, "tokens_output"),
        tokens_reasoning: count(&session_row, "tokens_reasoning"),
        tokens_cache_read: count(&session_row, "tokens_cache_read"),
        tokens_cache_write: count(&session_row, "tokens_cache_write"),
    })
}

pub(super) fn series(
    database: &ReadonlyDatabase,
    sql: &str,
    request: &MiningRequest,
) -> Vec<MiningSeriesPoint> {
    database
        .query(sql, params_from_iter(binds(request)))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let label = string_value(row.get("label"))?.to_string();
            let value = row.get("value").cloned().unwrap_or(Value::Null);
            Some(MiningSeriesPoint { label, value })
        })
        .collect()
}

pub(super) fn activity(
    database: &ReadonlyDatabase,
    request: &MiningRequest,
) -> Vec<MiningActivityDay> {
    let rows = database
        .query(
            &format!(
                "SELECT date(m.time_created / 1000, 'unixepoch') AS day, \
                 CAST(strftime('%H', m.time_created / 1000, 'unixepoch') AS INTEGER) AS hour, \
                 COUNT(*) AS value \
                 FROM message m JOIN session s ON m.session_id = s.id \
                 WHERE {JOINED_FILTER} GROUP BY day, hour ORDER BY day, hour"
            ),
            params_from_iter(binds(request)),
        )
        .unwrap_or_default();
    let mut days: BTreeMap<String, [u64; HOURS_PER_DAY]> = BTreeMap::new();
    for row in rows {
        let Some(day) = string_value(row.get("day")).map(str::to_string) else {
            continue;
        };
        let hour = usize::try_from(count_value(row.get("hour")).unwrap_or(0)).unwrap_or(0);
        let value = count_value(row.get("value")).unwrap_or(0);
        if let Some(slot) = days
            .entry(day)
            .or_default()
            .get_mut(hour.min(HOURS_PER_DAY - 1))
        {
            *slot = value;
        }
    }
    days.into_iter()
        .map(|(label, values)| MiningActivityDay {
            label,
            values: values.to_vec(),
        })
        .collect()
}

pub(super) fn sessions(database: &ReadonlyDatabase, request: &MiningRequest) -> Vec<MinedSession> {
    let mut binds = binds(request);
    binds.push(SqlValue::Integer(SESSIONS_CEILING));
    database
        .query(
            &format!(
                "SELECT id, title, directory, ROUND(cost, 4) AS cost_usd, \
                 tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, \
                 json_extract(model, '$.id') AS model, agent, time_created, time_updated \
                 FROM session WHERE {SESSION_FILTER} ORDER BY time_updated DESC LIMIT ?"
            ),
            params_from_iter(binds),
        )
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let count = |key: &str| count_value(row.get(key)).unwrap_or(0);
            Some(MinedSession {
                id: string_value(row.get("id"))?.to_string(),
                harness: HarnessKind::OpenCode.as_str().into(),
                title: string_value(row.get("title"))
                    .unwrap_or_default()
                    .to_string(),
                directory: string_value(row.get("directory"))
                    .unwrap_or_default()
                    .to_string(),
                cost_usd: number_value(row.get("cost_usd")).unwrap_or(0.0),
                tokens_input: count("tokens_input"),
                tokens_output: count("tokens_output"),
                tokens_reasoning: count("tokens_reasoning"),
                tokens_cache_read: count("tokens_cache_read"),
                model: string_value(row.get("model")).map(str::to_string),
                agent: string_value(row.get("agent")).map(str::to_string),
                time_created: count("time_created"),
                time_updated: count("time_updated"),
            })
        })
        .collect()
}
