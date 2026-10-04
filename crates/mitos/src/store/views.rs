use std::collections::HashSet;

use anyhow::Result;
use libsql::{Row, Value, params};

use super::Store;
use super::threads::thread_from_row;
use crate::domain::{EventKind, ThreadSummary, ThreadUsage};

/// The events a thread records about itself before anything is said.
const SETUP_EVENTS: [EventKind; 4] = [
    EventKind::ThreadCreated,
    EventKind::HarnessBound,
    EventKind::HarnessUnbound,
    EventKind::ModeChanged,
];

/// The figures that accumulate over a thread; the rest of a snapshot is a
/// point-in-time reading.
#[derive(Clone, Copy)]
enum Cumulative {
    InputTokens,
    OutputTokens,
    CachedInputTokens,
    CostUsd,
}

impl Cumulative {
    fn column(self) -> &'static str {
        match self {
            Self::InputTokens => "input_tokens",
            Self::OutputTokens => "output_tokens",
            Self::CachedInputTokens => "cached_input_tokens",
            Self::CostUsd => "cost_usd",
        }
    }

    /// Hooks only report cost, so only that column has an observed source.
    fn observed_column(self) -> &'static str {
        match self {
            Self::CostUsd => "cost_usd",
            _ => "NULL",
        }
    }
}

struct Reading {
    observed_at: String,
    model: Option<String>,
    turns: Option<u32>,
    context_used_tokens: Option<u64>,
    context_limit_tokens: Option<u64>,
}

#[derive(Default)]
struct PlanLimits {
    five_hour_percent: Option<f64>,
    five_hour_resets_at: Option<String>,
    week_percent: Option<f64>,
    week_resets_at: Option<String>,
}

fn count(row: &Row, index: i32) -> Result<Option<u64>> {
    Ok(row
        .get::<Option<i64>>(index)?
        .and_then(|value| u64::try_from(value).ok()))
}

fn reading_from_row(row: &Row) -> Result<Reading> {
    Ok(Reading {
        observed_at: row.get(0)?,
        model: row.get(1)?,
        turns: row
            .get::<Option<i64>>(2)?
            .and_then(|value| u32::try_from(value).ok()),
        context_used_tokens: count(row, 3)?,
        context_limit_tokens: count(row, 4)?,
    })
}

fn limits_from_row(row: &Row) -> Result<PlanLimits> {
    Ok(PlanLimits {
        five_hour_percent: row.get(0)?,
        five_hour_resets_at: row.get(1)?,
        week_percent: row.get(2)?,
        week_resets_at: row.get(3)?,
    })
}

/// SQLite sums integers as integers and reals as reals.
fn total_from_row(row: &Row) -> Result<Option<f64>> {
    #[allow(clippy::cast_precision_loss)]
    Ok(match row.get_value(0)? {
        Value::Integer(total) => Some(total as f64),
        Value::Real(total) => Some(total),
        _ => None,
    })
}

impl Store {
    /// A workspace's threads, newest first, without the archived ones.
    pub fn thread_summaries(&self, workspace_key: &str) -> Result<Vec<ThreadSummary>> {
        self.query_all(
            "SELECT threads.id, threads.workspace_id, threads.status, threads.mode, \
                    threads.active_harness, threads.native_session, threads.last_event_seq, \
                    threads.created_at, threads.updated_at, \
                    (SELECT content FROM thread_events \
                     WHERE thread_id = threads.id AND kind = 'user_message' AND content IS NOT NULL \
                     ORDER BY seq ASC LIMIT 1) \
             FROM threads JOIN workspaces ON workspaces.id = threads.workspace_id \
             WHERE workspaces.workspace_key = ?1 AND threads.status != 'archived' \
             ORDER BY threads.updated_at DESC",
            params![workspace_key],
            |row| {
                Ok(ThreadSummary {
                    thread: thread_from_row(row)?,
                    opening_message: row.get(9)?,
                })
            },
        )
    }

    /// Messages the user sent in a workspace, newest first, each text once.
    pub fn recent_user_messages(&self, workspace_key: &str, limit: usize) -> Result<Vec<String>> {
        let contents = self.query_all(
            "SELECT thread_events.content FROM thread_events \
             JOIN threads ON threads.id = thread_events.thread_id \
             JOIN workspaces ON workspaces.id = threads.workspace_id \
             WHERE workspaces.workspace_key = ?1 \
               AND thread_events.kind = 'user_message' \
               AND thread_events.role = 'user' \
               AND thread_events.content IS NOT NULL \
             ORDER BY thread_events.created_at DESC, thread_events.seq DESC",
            params![workspace_key],
            |row| Ok(row.get::<String>(0)?),
        )?;
        let mut seen = HashSet::new();
        let mut messages = Vec::new();
        for content in contents {
            if messages.len() >= limit {
                break;
            }
            let text = content.trim();
            if !text.is_empty() && seen.insert(text.to_string()) {
                messages.push(text.to_string());
            }
        }
        Ok(messages)
    }

    /// True only when the thread exists and has recorded nothing beyond its
    /// own setup events.
    pub fn is_thread_empty(&self, thread_id: &str) -> Result<bool> {
        let exists = self
            .query_optional(
                "SELECT 1 FROM threads WHERE id = ?1",
                params![thread_id],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .is_some();
        let setup = SETUP_EVENTS
            .iter()
            .map(|kind| format!("'{}'", kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let substantive = self
            .query_optional(
                &format!(
                    "SELECT 1 FROM thread_events WHERE thread_id = ?1 \
                     AND kind NOT IN ({setup}) LIMIT 1"
                ),
                params![thread_id],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .is_some();
        Ok(exists && !substantive)
    }

    /// Latest context and model readings (a live hook reading beats an older
    /// snapshot), the thread's cumulative tokens and cost, plus account-wide
    /// rate windows for the harness. A harness with only hook data still has
    /// usage.
    pub fn latest_usage(&self, thread_id: &str, harness: &str) -> Result<Option<ThreadUsage>> {
        let snapshot = self.query_optional(
            "SELECT observed_at, model, turns, context_used_tokens, context_limit_tokens \
             FROM usage_snapshots WHERE thread_id = ?1 AND harness = ?2 \
             ORDER BY observed_at DESC, rowid DESC LIMIT 1",
            params![thread_id, harness],
            reading_from_row,
        )?;
        let observation = self.query_optional(
            "SELECT observed_at, model, NULL, context_used_tokens, context_limit_tokens \
             FROM hook_observations WHERE thread_id = ?1 AND harness = ?2 \
             ORDER BY observed_at DESC LIMIT 1",
            params![thread_id, harness],
            reading_from_row,
        )?;
        if snapshot.is_none() && observation.is_none() {
            return Ok(None);
        }
        let limits = self
            .query_optional(
                "SELECT plan_five_hour_percent, plan_five_hour_resets_at, \
                        plan_week_percent, plan_week_resets_at \
                 FROM harness_plan_usage WHERE harness = ?1",
                params![harness],
                limits_from_row,
            )?
            .unwrap_or_default();

        let live_reading = observation.as_ref().is_some_and(|observed| {
            observed.context_limit_tokens.is_some()
                && snapshot
                    .as_ref()
                    .is_none_or(|snapshot| observed.observed_at > snapshot.observed_at)
        });
        let context = if live_reading {
            observation.as_ref()
        } else {
            snapshot.as_ref().or(observation.as_ref())
        };
        let observed_at = [snapshot.as_ref(), observation.as_ref()]
            .into_iter()
            .flatten()
            .map(|reading| reading.observed_at.clone())
            .max()
            .unwrap_or_default();

        Ok(Some(ThreadUsage {
            harness: harness.to_string(),
            observed_at,
            input_tokens: self.thread_tokens(thread_id, Cumulative::InputTokens)?,
            output_tokens: self.thread_tokens(thread_id, Cumulative::OutputTokens)?,
            cached_input_tokens: self.thread_tokens(thread_id, Cumulative::CachedInputTokens)?,
            cost_usd: self.thread_total(thread_id, Cumulative::CostUsd)?,
            context_used_tokens: context.and_then(|reading| reading.context_used_tokens),
            context_limit_tokens: context.and_then(|reading| reading.context_limit_tokens),
            model: context
                .and_then(|reading| reading.model.clone())
                .or_else(|| snapshot.as_ref().and_then(|reading| reading.model.clone()))
                .or_else(|| {
                    observation
                        .as_ref()
                        .and_then(|reading| reading.model.clone())
                }),
            turns: snapshot.as_ref().and_then(|reading| reading.turns),
            plan_five_hour_percent: limits.five_hour_percent,
            plan_five_hour_resets_at: limits.five_hour_resets_at,
            plan_week_percent: limits.week_percent,
            plan_week_resets_at: limits.week_resets_at,
        }))
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn thread_tokens(&self, thread_id: &str, figure: Cumulative) -> Result<Option<u64>> {
        Ok(self
            .thread_total(thread_id, figure)?
            .map(|total| total.max(0.0) as u64))
    }

    /// One figure summed across the whole thread, any harness, from the most
    /// precise source each harness has: streamed turns, else hook observations
    /// (one running total per native session), else the handoff snapshot (a
    /// session total). A turn may report a running figure many times, so only
    /// its last report counts. `None` when nothing reported it.
    fn thread_total(&self, thread_id: &str, figure: Cumulative) -> Result<Option<f64>> {
        let column = figure.column();
        let observed = figure.observed_column();
        Ok(self
            .query_optional(
                &format!(
                    "WITH reported AS ( \
                       SELECT rowid AS rid, harness, turn_id, {column} AS value \
                       FROM usage_snapshots WHERE thread_id = ?1 AND {column} IS NOT NULL \
                     ), \
                     streamed AS ( \
                       SELECT value, harness FROM reported AS r \
                       WHERE turn_id IS NOT NULL \
                         AND rid = (SELECT MAX(rid) FROM reported WHERE turn_id = r.turn_id) \
                     ), \
                     seen AS ( \
                       SELECT harness, {observed} AS value FROM hook_observations \
                       WHERE thread_id = ?1 AND {observed} IS NOT NULL \
                     ), \
                     observed AS ( \
                       SELECT value, harness FROM seen \
                       WHERE harness NOT IN (SELECT harness FROM streamed) \
                     ), \
                     collected AS ( \
                       SELECT value FROM reported AS r \
                       WHERE turn_id IS NULL \
                         AND harness NOT IN (SELECT harness FROM streamed) \
                         AND harness NOT IN (SELECT harness FROM observed) \
                         AND rid = (SELECT MAX(rid) FROM reported \
                                    WHERE turn_id IS NULL AND harness = r.harness) \
                     ) \
                     SELECT CASE WHEN (SELECT COUNT(*) FROM reported) + (SELECT COUNT(*) FROM seen) = 0 THEN NULL \
                            ELSE COALESCE((SELECT SUM(value) FROM streamed), 0) \
                               + COALESCE((SELECT SUM(value) FROM observed), 0) \
                               + COALESCE((SELECT SUM(value) FROM collected), 0) \
                            END AS total"
                ),
                params![thread_id],
                total_from_row,
            )?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use libsql::params;

    use crate::store::Store;
    use crate::store::fixtures::test_store;

    const START: &str = "2026-01-01T00:00:00Z";

    fn workspace(store: &Store, id: &str, key: &str) {
        store
            .execute(
                "INSERT INTO workspaces (id, root, workspace_key, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, format!("/{id}"), key, START],
            )
            .unwrap();
    }

    fn thread(store: &Store, id: &str, workspace_id: &str, updated: &str) {
        store
            .execute(
                "INSERT INTO threads (id, workspace_id, status, active_harness, last_event_seq, created_at, updated_at) \
                 VALUES (?1, ?2, 'active', 'codex', 0, ?3, ?3)",
                params![id, workspace_id, updated],
            )
            .unwrap();
    }

    fn event(store: &Store, thread_id: &str, seq: i64, kind: &str, content: Option<&str>) {
        store
            .execute(
                "INSERT INTO thread_events (thread_id, seq, kind, role, content, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    thread_id,
                    seq,
                    kind,
                    (kind == "user_message").then_some("user"),
                    content,
                    START
                ],
            )
            .unwrap();
    }

    /// A fixed ascending clock so "latest" is the order of insertion.
    fn snapshot(
        store: &Store,
        serial: &mut u32,
        thread_id: &str,
        harness: &str,
        turn_id: Option<&str>,
        counts: [Option<i64>; 3],
        cost: Option<f64>,
    ) {
        *serial += 1;
        store
            .execute(
                "INSERT INTO usage_snapshots \
                 (id, thread_id, harness, turn_id, observed_at, input_tokens, output_tokens, cached_input_tokens, cost_usd) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    format!("snap-{serial}"),
                    thread_id,
                    harness,
                    turn_id,
                    format!("2026-01-01T00:00:{:02}Z", *serial),
                    counts[0],
                    counts[1],
                    counts[2],
                    cost
                ],
            )
            .unwrap();
    }

    fn cost(
        store: &Store,
        serial: &mut u32,
        thread_id: &str,
        harness: &str,
        turn: Option<&str>,
        value: Option<f64>,
    ) {
        snapshot(store, serial, thread_id, harness, turn, [None; 3], value);
    }

    fn tokens(
        store: &Store,
        serial: &mut u32,
        thread_id: &str,
        harness: &str,
        turn: Option<&str>,
        counts: [Option<i64>; 3],
    ) {
        snapshot(store, serial, thread_id, harness, turn, counts, None);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn observe(store: &Store, thread_id: &str, harness: &str, session: &str, fields: Observed) {
        store
            .execute(
                "INSERT INTO hook_observations \
                 (harness, native_session, thread_id, model, cost_usd, context_used_tokens, context_limit_tokens, observed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    harness,
                    session,
                    thread_id,
                    fields.model,
                    fields.cost,
                    fields.context_used,
                    fields.context_limit,
                    fields.at.unwrap_or("2026-01-01T00:10:00Z")
                ],
            )
            .unwrap();
    }

    #[derive(Default)]
    struct Observed {
        cost: Option<f64>,
        context_used: Option<i64>,
        context_limit: Option<i64>,
        model: Option<&'static str>,
        at: Option<&'static str>,
    }

    fn usage_cost(store: &Store, thread_id: &str, harness: &str) -> Option<f64> {
        store
            .latest_usage(thread_id, harness)
            .unwrap()
            .and_then(|usage| usage.cost_usd)
    }

    fn close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("a cost");
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    fn finish(root: std::path::PathBuf) {
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lists_only_threads_from_the_launched_workspace_with_their_opening_message() {
        let (store, root) = test_store();
        workspace(&store, "ws-a", "workspace-a");
        workspace(&store, "ws-b", "workspace-b");
        thread(&store, "a", "ws-a", START);
        thread(&store, "b", "ws-b", "2026-01-02T00:00:00Z");
        event(&store, "a", 1, "user_message", Some("continue auth work"));
        event(&store, "a", 2, "user_message", Some("later message"));

        let listed = store.thread_summaries("workspace-a").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].thread.id, "a");
        assert_eq!(listed[0].thread.workspace_id, "ws-a");
        assert_eq!(
            listed[0].opening_message.as_deref(),
            Some("continue auth work")
        );
        assert_eq!(store.thread_summaries("workspace-b").unwrap().len(), 1);
        assert!(store.thread_summaries("workspace-c").unwrap().is_empty());
        finish(root);
    }

    #[test]
    fn archived_threads_are_not_listed_and_the_newest_comes_first() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "old", "ws", START);
        thread(&store, "new", "ws", "2026-01-03T00:00:00Z");
        thread(&store, "gone", "ws", "2026-01-04T00:00:00Z");
        store
            .execute(
                "UPDATE threads SET status = 'archived' WHERE id = ?1",
                params!["gone"],
            )
            .unwrap();
        store
            .execute(
                "UPDATE threads SET mode = 'plan' WHERE id = ?1",
                params!["new"],
            )
            .unwrap();

        let listed = store.thread_summaries("workspace").unwrap();
        let ids: Vec<_> = listed
            .iter()
            .map(|summary| summary.thread.id.as_str())
            .collect();
        assert_eq!(ids, ["new", "old"]);
        assert_eq!(listed[0].thread.mode.as_str(), "plan");
        assert_eq!(listed[0].opening_message, None);
        finish(root);
    }

    #[test]
    fn a_summary_serializes_the_thread_fields_beside_the_opening_message() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        event(&store, "t", 1, "user_message", Some("hello"));
        let listed = store.thread_summaries("workspace").unwrap();
        let json = serde_json::to_value(&listed[0]).unwrap();
        assert_eq!(json["id"], "t");
        assert_eq!(json["status"], "active");
        assert_eq!(json["opening_message"], "hello");
        assert!(json["native_session"].is_null());
        finish(root);
    }

    #[test]
    fn user_history_is_newest_first_with_each_text_once_up_to_the_limit() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        for (seq, text) in [
            (1, "first"),
            (2, "  repeat  "),
            (3, "repeat"),
            (4, "last"),
            (5, "   "),
        ] {
            event(&store, "t", seq, "user_message", Some(text));
        }
        event(&store, "t", 6, "assistant_message", Some("not the user"));
        assert_eq!(
            store.recent_user_messages("workspace", 10).unwrap(),
            ["last", "repeat", "first"]
        );
        assert_eq!(
            store.recent_user_messages("workspace", 2).unwrap(),
            ["last", "repeat"]
        );
        assert!(store.recent_user_messages("other", 5).unwrap().is_empty());
        finish(root);
    }

    #[test]
    fn a_thread_is_empty_until_it_records_something_beyond_its_setup_events() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "fresh", "ws", START);
        thread(&store, "used", "ws", START);
        for id in ["fresh", "used"] {
            event(&store, id, 1, "thread_created", None);
            event(&store, id, 2, "harness_bound", None);
        }
        event(&store, "used", 3, "user_message", None);
        assert!(store.is_thread_empty("fresh").unwrap());
        assert!(!store.is_thread_empty("used").unwrap());
        assert!(!store.is_thread_empty("missing").unwrap());
        finish(root);
    }

    #[test]
    fn reads_latest_context_token_and_rate_usage() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "session", "ws", START);
        store
            .execute(
                "INSERT INTO usage_snapshots (id, thread_id, harness, observed_at, input_tokens, output_tokens, cached_input_tokens, cost_usd, context_used_tokens, context_limit_tokens, model, turns) \
                 VALUES ('u', 'session', 'codex', ?1, 100, 50, 25, 0.0421, 64000, 128000, 'gpt', 2)",
                params![START],
            )
            .unwrap();
        store
            .execute(
                "INSERT INTO harness_plan_usage (harness, plan_five_hour_percent, observed_at) VALUES ('codex', 42.5, ?1)",
                params![START],
            )
            .unwrap();
        let usage = store.latest_usage("session", "codex").unwrap().unwrap();
        assert_eq!(usage.context_used_tokens, Some(64_000));
        assert_eq!(usage.context_limit_tokens, Some(128_000));
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(usage.turns, Some(2));
        assert_eq!(usage.model.as_deref(), Some("gpt"));
        assert_eq!(usage.plan_five_hour_percent, Some(42.5));
        assert_eq!(usage.plan_week_percent, None);
        close(usage.cost_usd, 0.0421);
        assert!(store.latest_usage("session", "claude").unwrap().is_none());
        finish(root);
    }

    #[test]
    fn cost_is_cumulative_across_turns_counting_each_turns_last_report_once() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        for (turn, value) in [
            ("turn-1", 0.1),
            ("turn-1", 0.3),
            ("turn-1", 0.5),
            ("turn-2", 0.25),
        ] {
            cost(
                &store,
                &mut serial,
                "t",
                "opencode",
                Some(turn),
                Some(value),
            );
        }
        close(usage_cost(&store, "t", "opencode"), 0.75);
        finish(root);
    }

    #[test]
    fn cost_adds_up_across_harnesses_within_one_thread() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(
            &store,
            &mut serial,
            "t",
            "claude",
            Some("turn-1"),
            Some(1.0),
        );
        cost(
            &store,
            &mut serial,
            "t",
            "opencode",
            Some("turn-2"),
            Some(2.0),
        );
        close(usage_cost(&store, "t", "opencode"), 3.0);
        close(usage_cost(&store, "t", "claude"), 3.0);
        finish(root);
    }

    #[test]
    fn a_handoff_total_does_not_double_count_a_harness_that_streamed_its_cost() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(
            &store,
            &mut serial,
            "t",
            "opencode",
            Some("turn-1"),
            Some(0.5),
        );
        cost(&store, &mut serial, "t", "opencode", None, Some(0.5));
        close(usage_cost(&store, "t", "opencode"), 0.5);
        finish(root);
    }

    #[test]
    fn a_handoff_only_harness_contributes_its_latest_session_total() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(
            &store,
            &mut serial,
            "t",
            "claude",
            Some("turn-1"),
            Some(1.0),
        );
        cost(&store, &mut serial, "t", "hermes", None, Some(0.2));
        cost(&store, &mut serial, "t", "hermes", None, Some(0.4));
        close(usage_cost(&store, "t", "hermes"), 1.4);
        finish(root);
    }

    #[test]
    fn tokens_are_cumulative_across_turns_and_harnesses_each_turn_counted_once() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        tokens(
            &store,
            &mut serial,
            "t",
            "claude",
            Some("turn-1"),
            [Some(10), Some(1), None],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "claude",
            Some("turn-1"),
            [Some(100), Some(20), Some(5)],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "codex",
            Some("turn-2"),
            [Some(200), Some(30), Some(50)],
        );
        let usage = store.latest_usage("t", "codex").unwrap().unwrap();
        assert_eq!(usage.input_tokens, Some(300));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.cached_input_tokens, Some(55));
        finish(root);
    }

    #[test]
    fn a_turns_tokens_come_from_the_last_report_that_carried_them() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        tokens(
            &store,
            &mut serial,
            "t",
            "hermes",
            Some("turn-1"),
            [None; 3],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "hermes",
            Some("turn-1"),
            [Some(40), Some(8), None],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "hermes",
            Some("turn-1"),
            [None; 3],
        );
        let usage = store.latest_usage("t", "hermes").unwrap().unwrap();
        assert_eq!(usage.input_tokens, Some(40));
        assert_eq!(usage.output_tokens, Some(8));
        finish(root);
    }

    #[test]
    fn a_handoff_token_total_does_not_double_count_streamed_turns() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        tokens(
            &store,
            &mut serial,
            "t",
            "opencode",
            Some("turn-1"),
            [Some(100), Some(10), None],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "opencode",
            None,
            [Some(100), Some(10), None],
        );
        tokens(
            &store,
            &mut serial,
            "t",
            "hermes",
            None,
            [Some(7), Some(3), None],
        );
        let usage = store.latest_usage("t", "opencode").unwrap().unwrap();
        assert_eq!(usage.input_tokens, Some(107));
        assert_eq!(usage.output_tokens, Some(13));
        finish(root);
    }

    #[test]
    fn tokens_and_cost_are_none_until_something_reports_them() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        tokens(&store, &mut serial, "t", "codex", Some("turn-1"), [None; 3]);
        let usage = store.latest_usage("t", "codex").unwrap().unwrap();
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.cached_input_tokens, None);
        assert_eq!(usage.cost_usd, None);
        finish(root);
    }

    #[test]
    fn hook_observed_session_costs_add_up_across_a_threads_native_sessions() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        observe(
            &store,
            "t",
            "claude",
            "s1",
            Observed {
                cost: Some(1.5),
                ..Observed::default()
            },
        );
        observe(
            &store,
            "t",
            "claude",
            "s2",
            Observed {
                cost: Some(0.5),
                ..Observed::default()
            },
        );
        close(usage_cost(&store, "t", "claude"), 2.0);
        finish(root);
    }

    #[test]
    fn an_observed_cost_replaces_a_handoff_snapshots_for_the_same_harness() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(&store, &mut serial, "t", "opencode", None, Some(5.0));
        observe(
            &store,
            "t",
            "opencode",
            "s1",
            Observed {
                cost: Some(2.0),
                ..Observed::default()
            },
        );
        close(usage_cost(&store, "t", "opencode"), 2.0);
        finish(root);
    }

    #[test]
    fn streamed_turn_costs_win_over_a_hook_observation_of_the_same_harness() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(
            &store,
            &mut serial,
            "t",
            "opencode",
            Some("turn-1"),
            Some(1.0),
        );
        observe(
            &store,
            "t",
            "opencode",
            "s1",
            Observed {
                cost: Some(1.0),
                ..Observed::default()
            },
        );
        close(usage_cost(&store, "t", "opencode"), 1.0);
        finish(root);
    }

    #[test]
    fn a_harness_with_only_hook_data_still_reports_usage_with_its_live_context() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        observe(
            &store,
            "t",
            "claude",
            "s1",
            Observed {
                cost: Some(0.75),
                context_used: Some(16_000),
                context_limit: Some(200_000),
                model: Some("opus"),
                ..Observed::default()
            },
        );
        let usage = store.latest_usage("t", "claude").unwrap().unwrap();
        close(usage.cost_usd, 0.75);
        assert_eq!(usage.context_used_tokens, Some(16_000));
        assert_eq!(usage.context_limit_tokens, Some(200_000));
        assert_eq!(usage.model.as_deref(), Some("opus"));
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.turns, None);
        finish(root);
    }

    #[test]
    fn a_newer_hook_reading_beats_an_older_snapshots_context_and_an_older_one_loses() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        store
            .execute(
                "INSERT INTO usage_snapshots (id, thread_id, harness, observed_at, context_used_tokens, context_limit_tokens) \
                 VALUES ('snap-ctx', 't', 'claude', '2026-01-01T00:05:00Z', 1000, 200000)",
                (),
            )
            .unwrap();
        observe(
            &store,
            "t",
            "claude",
            "s1",
            Observed {
                context_used: Some(9000),
                context_limit: Some(200_000),
                at: Some("2026-01-01T00:10:00Z"),
                ..Observed::default()
            },
        );
        let usage = store.latest_usage("t", "claude").unwrap().unwrap();
        assert_eq!(usage.context_used_tokens, Some(9000));
        assert_eq!(usage.observed_at, "2026-01-01T00:10:00Z");

        store
            .execute(
                "DELETE FROM hook_observations WHERE thread_id = ?1",
                params!["t"],
            )
            .unwrap();
        observe(
            &store,
            "t",
            "claude",
            "s1",
            Observed {
                context_used: Some(7),
                context_limit: Some(200_000),
                at: Some("2026-01-01T00:01:00Z"),
                ..Observed::default()
            },
        );
        let usage = store.latest_usage("t", "claude").unwrap().unwrap();
        assert_eq!(usage.context_used_tokens, Some(1000));
        assert_eq!(usage.observed_at, "2026-01-01T00:05:00Z");
        finish(root);
    }

    #[test]
    fn nothing_leaks_between_threads() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "a", "ws", START);
        thread(&store, "b", "ws", START);
        observe(
            &store,
            "a",
            "claude",
            "s1",
            Observed {
                cost: Some(9.0),
                context_limit: Some(200_000),
                ..Observed::default()
            },
        );
        assert!(store.latest_usage("b", "claude").unwrap().is_none());

        let mut serial = 0;
        cost(
            &store,
            &mut serial,
            "a",
            "claude",
            Some("turn-1"),
            Some(9.0),
        );
        cost(
            &store,
            &mut serial,
            "b",
            "claude",
            Some("turn-2"),
            Some(1.0),
        );
        close(usage_cost(&store, "b", "claude"), 1.0);
        finish(root);
    }

    #[test]
    fn cost_is_none_until_something_reports_one() {
        let (store, root) = test_store();
        workspace(&store, "ws", "workspace");
        thread(&store, "t", "ws", START);
        let mut serial = 0;
        cost(&store, &mut serial, "t", "codex", Some("turn-1"), None);
        assert_eq!(usage_cost(&store, "t", "codex"), None);
        finish(root);
    }
}
