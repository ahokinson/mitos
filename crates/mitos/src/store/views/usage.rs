use anyhow::Result;
use libsql::{Row, Value, params};

use super::super::Store;
use super::super::queries::text;
use crate::domain::{HarnessKind, ThreadId, ThreadUsage, Timestamp, now};

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
    observed_at: Timestamp,
    model: Option<String>,
    provider: Option<String>,
    plan: Option<String>,
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
    month_percent: Option<f64>,
    month_resets_at: Option<String>,
}

fn count(row: &Row, index: i32) -> Result<Option<u64>> {
    Ok(row
        .get::<Option<i64>>(index)?
        .and_then(|value| u64::try_from(value).ok()))
}

fn reading_from_row(row: &Row) -> Result<Reading> {
    Ok(Reading {
        observed_at: text(row, 0)?,
        model: row.get(1)?,
        provider: row.get(2)?,
        plan: row.get(3)?,
        turns: row
            .get::<Option<i64>>(4)?
            .and_then(|value| u32::try_from(value).ok()),
        context_used_tokens: count(row, 5)?,
        context_limit_tokens: count(row, 6)?,
    })
}

fn limits_from_row(row: &Row) -> Result<PlanLimits> {
    Ok(PlanLimits {
        five_hour_percent: row.get(0)?,
        five_hour_resets_at: row.get(1)?,
        week_percent: row.get(2)?,
        week_resets_at: row.get(3)?,
        month_percent: row.get(4)?,
        month_resets_at: row.get(5)?,
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
    /// Latest context and model readings (a live hook reading beats an older
    /// snapshot), the thread's cumulative tokens and cost, plus account-wide
    /// rate windows for the harness. A harness with only hook data still has
    /// usage.
    pub fn latest_usage(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
    ) -> Result<Option<ThreadUsage>> {
        let snapshot = self.query_optional(
            "SELECT observed_at, model, provider, plan, turns, context_used_tokens, context_limit_tokens \
             FROM usage_snapshots WHERE thread_id = ?1 AND harness = ?2 \
             ORDER BY observed_at DESC, rowid DESC LIMIT 1",
            params![thread_id.as_str(), harness.as_str()],
            reading_from_row,
        )?;
        let observation = self.query_optional(
            "SELECT observed_at, model, provider, plan, NULL, context_used_tokens, context_limit_tokens \
             FROM hook_observations WHERE thread_id = ?1 AND harness = ?2 \
             ORDER BY observed_at DESC LIMIT 1",
            params![thread_id.as_str(), harness.as_str()],
            reading_from_row,
        )?;
        if snapshot.is_none() && observation.is_none() {
            return Ok(None);
        }
        let plan = [snapshot.as_ref(), observation.as_ref()]
            .into_iter()
            .flatten()
            .max_by_key(|reading| &reading.observed_at)
            .and_then(|reading| reading.plan.clone());
        let limits = match plan.as_deref() {
            Some(plan) => self
                .query_optional(
                    "SELECT plan_five_hour_percent, plan_five_hour_resets_at, \
                         plan_week_percent, plan_week_resets_at, \
                         plan_month_percent, plan_month_resets_at \
                  FROM plan_usage WHERE plan = ?1",
                    params![plan],
                    limits_from_row,
                )?
                .unwrap_or_default(),
            None => PlanLimits::default(),
        };

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
            .unwrap_or_else(now);

        Ok(Some(ThreadUsage {
            harness,
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
            provider: context
                .and_then(|reading| reading.provider.clone())
                .or_else(|| {
                    snapshot
                        .as_ref()
                        .and_then(|reading| reading.provider.clone())
                })
                .or_else(|| {
                    observation
                        .as_ref()
                        .and_then(|reading| reading.provider.clone())
                }),
            plan,
            plan_label: None,
            turns: snapshot.as_ref().and_then(|reading| reading.turns),
            plan_five_hour_percent: limits.five_hour_percent,
            plan_five_hour_resets_at: limits.five_hour_resets_at,
            plan_week_percent: limits.week_percent,
            plan_week_resets_at: limits.week_resets_at,
            plan_month_percent: limits.month_percent,
            plan_month_resets_at: limits.month_resets_at,
        }))
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn thread_tokens(&self, thread_id: &ThreadId, figure: Cumulative) -> Result<Option<u64>> {
        Ok(self
            .thread_total(thread_id, figure)?
            .map(|total| total.max(0.0) as u64))
    }

    /// One figure summed across the whole thread, any harness, from the most
    /// precise source each harness has: streamed turns, else hook observations
    /// (one running total per native session), else the handoff snapshot (a
    /// session total). A turn may report a running figure many times, so only
    /// its last report counts. `None` when nothing reported it.
    fn thread_total(&self, thread_id: &ThreadId, figure: Cumulative) -> Result<Option<f64>> {
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
                params![thread_id.as_str()],
                total_from_row,
            )?
            .flatten())
    }
}
