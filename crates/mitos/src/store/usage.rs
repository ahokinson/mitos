use anyhow::Result;
use libsql::params;

use super::Store;
use crate::domain::{HarnessKind, ThreadId, TurnId, UsageSnapshot, id, now};

#[derive(Clone, Debug, Default)]
pub struct UsageFields {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub context_used_tokens: Option<u64>,
    pub context_limit_tokens: Option<u64>,
    pub model: Option<String>,
    pub turns: Option<u32>,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Default)]
pub struct PlanUsageFields {
    pub plan_five_hour_percent: Option<f64>,
    pub plan_five_hour_resets_at: Option<String>,
    pub plan_week_percent: Option<f64>,
    pub plan_week_resets_at: Option<String>,
}

fn sql_count(value: Option<u64>) -> Option<i64> {
    value.and_then(|value| i64::try_from(value).ok())
}

impl Store {
    pub fn record_usage_snapshot(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
        turn_id: Option<&TurnId>,
        fields: UsageFields,
    ) -> Result<UsageSnapshot> {
        let snapshot = UsageSnapshot {
            id: id(),
            thread_id: thread_id.clone(),
            harness,
            turn_id: turn_id.cloned(),
            observed_at: now(),
            input_tokens: fields.input_tokens,
            output_tokens: fields.output_tokens,
            cached_input_tokens: fields.cached_input_tokens,
            cost_usd: fields.cost_usd,
            context_used_tokens: fields.context_used_tokens,
            context_limit_tokens: fields.context_limit_tokens,
            model: fields.model,
            turns: fields.turns,
        };
        self.execute(
            "INSERT INTO usage_snapshots \
             (id, thread_id, harness, turn_id, observed_at, input_tokens, output_tokens, cached_input_tokens, cost_usd, context_used_tokens, context_limit_tokens, model, turns) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                snapshot.id.clone(),
                snapshot.thread_id.as_str(),
                snapshot.harness.as_str(),
                snapshot.turn_id.as_ref().map(TurnId::as_str),
                snapshot.observed_at.as_str(),
                sql_count(snapshot.input_tokens),
                sql_count(snapshot.output_tokens),
                sql_count(snapshot.cached_input_tokens),
                snapshot.cost_usd,
                sql_count(snapshot.context_used_tokens),
                sql_count(snapshot.context_limit_tokens),
                snapshot.model.clone(),
                snapshot.turns.map(i64::from)
            ],
        )?;
        Ok(snapshot)
    }

    pub fn upsert_plan_usage(&self, harness: HarnessKind, fields: PlanUsageFields) -> Result<()> {
        self.execute(
            "INSERT INTO harness_plan_usage \
             (harness, plan_five_hour_percent, plan_five_hour_resets_at, plan_week_percent, plan_week_resets_at, observed_at) \
             VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(harness) DO UPDATE SET \
               plan_five_hour_percent = excluded.plan_five_hour_percent, \
               plan_five_hour_resets_at = excluded.plan_five_hour_resets_at, \
               plan_week_percent = excluded.plan_week_percent, \
               plan_week_resets_at = excluded.plan_week_resets_at, \
               observed_at = excluded.observed_at",
            params![
                harness.as_str(),
                fields.plan_five_hour_percent,
                fields.plan_five_hour_resets_at,
                fields.plan_week_percent,
                fields.plan_week_resets_at,
                now().as_str()
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{PlanUsageFields, UsageFields};
    use crate::domain::HarnessKind;
    use crate::store::fixtures::{test_store, test_thread};
    use libsql::params;

    fn stored_cost(store: &crate::store::Store, snapshot_id: &str) -> Option<f64> {
        store
            .query_optional(
                "SELECT cost_usd FROM usage_snapshots WHERE id = ?1",
                params![snapshot_id],
                |row| Ok(row.get::<Option<f64>>(0)?),
            )
            .unwrap()
            .unwrap()
    }

    #[test]
    fn usage_snapshot_round_trips_cost() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "cost");
        let priced = store
            .record_usage_snapshot(
                &thread.id,
                HarnessKind::Claude,
                Some(&"turn-1".into()),
                UsageFields {
                    cost_usd: Some(0.0421),
                    ..UsageFields::default()
                },
            )
            .unwrap();
        let unpriced = store
            .record_usage_snapshot(&thread.id, HarnessKind::Codex, None, UsageFields::default())
            .unwrap();

        assert_eq!(stored_cost(&store, &priced.id), Some(0.0421));
        assert_eq!(stored_cost(&store, &unpriced.id), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plan_usage_upsert_overwrites_the_prior_observation() {
        let (store, root) = test_store();
        store
            .upsert_plan_usage(
                HarnessKind::Claude,
                PlanUsageFields {
                    plan_five_hour_percent: Some(10.0),
                    ..PlanUsageFields::default()
                },
            )
            .unwrap();
        store
            .upsert_plan_usage(
                HarnessKind::Claude,
                PlanUsageFields {
                    plan_five_hour_percent: Some(42.0),
                    ..PlanUsageFields::default()
                },
            )
            .unwrap();

        let usage = store.plan_usage(HarnessKind::Claude).unwrap().unwrap();
        assert_eq!(usage.plan_five_hour_percent, Some(42.0));
        fs::remove_dir_all(root).unwrap();
    }
}
