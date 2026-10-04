use anyhow::Result;
use libsql::params;

use super::Store;
use super::queries::text;
use super::usage::PlanUsageFields;
use crate::domain::{HookLastSeen, Observation, ThreadId, now};

fn sql_count(value: Option<u64>) -> Option<i64> {
    value.and_then(|value| i64::try_from(value).ok())
}

impl Store {
    /// Keeps the previous value of any field this observation leaves empty,
    /// so a hook that reports only a session id never erases the cost.
    pub fn record_observation(
        &self,
        observation: &Observation,
        thread_id: Option<&ThreadId>,
    ) -> Result<()> {
        self.execute(
            "INSERT INTO hook_observations \
             (harness, native_session, thread_id, cwd, model, event, cost_usd, context_used_tokens, context_limit_tokens, observed_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) \
             ON CONFLICT(harness, native_session) DO UPDATE SET \
               thread_id = COALESCE(excluded.thread_id, thread_id), \
               cwd = COALESCE(excluded.cwd, cwd), \
               model = COALESCE(excluded.model, model), \
               event = COALESCE(excluded.event, event), \
               cost_usd = COALESCE(excluded.cost_usd, cost_usd), \
               context_used_tokens = COALESCE(excluded.context_used_tokens, context_used_tokens), \
               context_limit_tokens = COALESCE(excluded.context_limit_tokens, context_limit_tokens), \
               observed_at = excluded.observed_at",
            params![
                observation.harness.as_str(),
                observation.native_session.clone().unwrap_or_default(),
                thread_id.map(ThreadId::as_str),
                observation.cwd.clone(),
                observation.model.clone(),
                observation.event.clone(),
                observation.cost_usd,
                sql_count(observation.context_used_tokens),
                sql_count(observation.context_limit_tokens),
                now().as_str()
            ],
        )?;
        if let Some(plan) = &observation.plan
            && (plan.five_hour_percent.is_some() || plan.week_percent.is_some())
        {
            self.upsert_plan_usage(
                observation.harness,
                PlanUsageFields {
                    plan_five_hour_percent: plan.five_hour_percent,
                    plan_five_hour_resets_at: plan.five_hour_resets_at.clone(),
                    plan_week_percent: plan.week_percent,
                    plan_week_resets_at: plan.week_resets_at.clone(),
                },
            )?;
        }
        Ok(())
    }

    pub fn hooks_last_seen(&self) -> Result<Vec<HookLastSeen>> {
        self.query_all(
            "SELECT harness, MAX(observed_at) FROM hook_observations GROUP BY harness",
            (),
            |row| {
                Ok(HookLastSeen {
                    harness: row.get::<String>(0)?.parse()?,
                    observed_at: text(row, 1)?,
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use libsql::params;

    use crate::domain::{HarnessKind, Observation, PlanWindows};
    use crate::store::fixtures::{test_store, test_thread};

    fn observation(session: &str) -> Observation {
        Observation {
            native_session: Some(session.into()),
            ..Observation::new(HarnessKind::Claude)
        }
    }

    fn cost_of(store: &crate::store::Store, harness: HarnessKind, session: &str) -> Option<f64> {
        store
            .query_optional(
                "SELECT cost_usd FROM hook_observations WHERE harness = ?1 AND native_session = ?2",
                params![harness.as_str(), session],
                |row| Ok(row.get::<Option<f64>>(0)?),
            )
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_later_observation_keeps_fields_it_leaves_empty() {
        let (store, root) = test_store();
        store
            .record_observation(
                &Observation {
                    cost_usd: Some(1.5),
                    model: Some("opus".into()),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();
        store
            .record_observation(
                &Observation {
                    event: Some("Stop".into()),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();

        assert_eq!(cost_of(&store, HarnessKind::Claude, "s1"), Some(1.5));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_new_cost_replaces_the_running_total() {
        let (store, root) = test_store();
        for cost in [0.5, 2.0] {
            store
                .record_observation(
                    &Observation {
                        cost_usd: Some(cost),
                        ..observation("s1")
                    },
                    None,
                )
                .unwrap();
        }

        assert_eq!(cost_of(&store, HarnessKind::Claude, "s1"), Some(2.0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sessions_are_tracked_separately_per_harness() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "hooks");
        store
            .record_observation(
                &Observation {
                    cost_usd: Some(1.0),
                    ..observation("s1")
                },
                Some(&thread.id),
            )
            .unwrap();
        store
            .record_observation(
                &Observation {
                    harness: HarnessKind::OpenCode,
                    cost_usd: Some(9.0),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();

        assert_eq!(store.hooks_last_seen().unwrap().len(), 2);
        assert_eq!(cost_of(&store, HarnessKind::Claude, "s1"), Some(1.0));
        assert_eq!(cost_of(&store, HarnessKind::OpenCode, "s1"), Some(9.0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plan_windows_land_in_account_wide_plan_usage() {
        let (store, root) = test_store();
        store
            .record_observation(
                &Observation {
                    plan: Some(PlanWindows {
                        five_hour_percent: Some(23.5),
                        five_hour_resets_at: Some("2030-01-01T00:00:00Z".into()),
                        ..PlanWindows::default()
                    }),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();

        let plan = store.plan_usage(HarnessKind::Claude).unwrap().unwrap();
        assert_eq!(plan.plan_five_hour_percent, Some(23.5));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_empty_plan_does_not_overwrite_a_known_one() {
        let (store, root) = test_store();
        store
            .record_observation(
                &Observation {
                    plan: Some(PlanWindows {
                        week_percent: Some(40.0),
                        ..PlanWindows::default()
                    }),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();
        store
            .record_observation(
                &Observation {
                    plan: Some(PlanWindows::default()),
                    ..observation("s1")
                },
                None,
            )
            .unwrap();

        let plan = store.plan_usage(HarnessKind::Claude).unwrap().unwrap();
        assert_eq!(plan.plan_week_percent, Some(40.0));
        fs::remove_dir_all(root).unwrap();
    }
}
