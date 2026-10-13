use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MiningReport {
    pub totals: MiningTotals,
    pub harnesses: Vec<MiningHarness>,
    pub cost_by_day: Vec<MiningSeriesPoint>,
    pub cost_by_project: Vec<MiningSeriesPoint>,
    pub cost_by_model: Vec<MiningSeriesPoint>,
    pub cost_by_agent: Vec<MiningSeriesPoint>,
    pub cost_by_harness: Vec<MiningSeriesPoint>,
    pub sessions_by_day: Vec<MiningSeriesPoint>,
    pub sessions_by_agent: Vec<MiningSeriesPoint>,
    pub sessions_by_model: Vec<MiningSeriesPoint>,
    pub tokens_by_day: Vec<MiningSeriesPoint>,
    pub tool_usage: Vec<MiningSeriesPoint>,
    pub tool_time: Vec<MiningSeriesPoint>,
    pub activity_by_day: Vec<MiningActivityDay>,
    pub sessions: Vec<MinedSession>,
    /// Estimated Go plan windows from mined spend; `None` without a
    /// minable `OpenCode` history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub est_windows: Option<EstWindows>,
}

/// One machine's share of the Go plan windows, estimated from the per-request
/// costs opencode records: each request's visible cost divided by its model's
/// monthly allowance, summed per window. `burn_*` fields are fractions of the
/// tier's monthly allowance, so a 5-hour figure of `0.4` is twice the 20%
/// the window allows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EstWindows {
    /// `go` or `plus`; the tier the allowances were drawn from.
    pub tier: String,
    pub burn_5h: f64,
    /// The oldest spend still inside the 5-hour window; the window eases
    /// from that moment plus five hours.
    pub oldest_5h_ms: Option<i64>,
    pub burn_week: f64,
    pub burn_month: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MiningTotals {
    pub sessions: u64,
    pub messages: u64,
    pub messages_user: u64,
    pub messages_assistant: u64,
    pub tool_calls: u64,
    pub patches: u64,
    pub files_patched: u64,
    pub forks: u64,
    pub cost_usd: f64,
    pub tool_seconds: f64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_reasoning: u64,
    pub tokens_cache_read: u64,
    pub tokens_cache_write: u64,
}

/// One harness's share of the merged report, so a view can compare harnesses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MiningHarness {
    pub label: String,
    pub sessions: u64,
    pub messages: u64,
    pub tool_calls: u64,
    pub patches: u64,
    pub cost_usd: f64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_cache_read: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MiningSeriesPoint {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MiningActivityDay {
    pub label: String,
    pub values: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MinedSession {
    pub id: String,
    pub harness: String,
    pub title: String,
    pub directory: String,
    pub cost_usd: f64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_reasoning: u64,
    pub tokens_cache_read: u64,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub time_created: u64,
    pub time_updated: u64,
}

impl MiningReport {
    fn is_blank(&self) -> bool {
        self == &Self::default()
    }

    /// Two providers' reports fold into one: totals add, series add per label,
    /// sessions concatenate newest first. Day series keep label order, which
    /// is date order; grouped series re-rank by value.
    pub fn merged(self, other: MiningReport) -> Self {
        if self.is_blank() {
            return other;
        }
        if other.is_blank() {
            return self;
        }
        let totals = MiningTotals {
            sessions: self.totals.sessions + other.totals.sessions,
            messages: self.totals.messages + other.totals.messages,
            messages_user: self.totals.messages_user + other.totals.messages_user,
            messages_assistant: self.totals.messages_assistant + other.totals.messages_assistant,
            tool_calls: self.totals.tool_calls + other.totals.tool_calls,
            patches: self.totals.patches + other.totals.patches,
            files_patched: self.totals.files_patched + other.totals.files_patched,
            forks: self.totals.forks + other.totals.forks,
            cost_usd: self.totals.cost_usd + other.totals.cost_usd,
            tool_seconds: self.totals.tool_seconds + other.totals.tool_seconds,
            tokens_input: self.totals.tokens_input + other.totals.tokens_input,
            tokens_output: self.totals.tokens_output + other.totals.tokens_output,
            tokens_reasoning: self.totals.tokens_reasoning + other.totals.tokens_reasoning,
            tokens_cache_read: self.totals.tokens_cache_read + other.totals.tokens_cache_read,
            tokens_cache_write: self.totals.tokens_cache_write + other.totals.tokens_cache_write,
        };
        let mut harnesses = self.harnesses;
        for harness in other.harnesses {
            match harnesses
                .iter_mut()
                .find(|existing| existing.label == harness.label)
            {
                Some(existing) => add_harness(existing, &harness),
                None => harnesses.push(harness),
            }
        }
        let mut sessions = self.sessions;
        sessions.extend(other.sessions);
        sessions.sort_by_key(|session| std::cmp::Reverse(session.time_updated));
        Self {
            totals,
            harnesses,
            est_windows: self.est_windows.or(other.est_windows),
            cost_by_day: merged_by_label(&self.cost_by_day, &other.cost_by_day, false),
            cost_by_project: merged_by_label(&self.cost_by_project, &other.cost_by_project, true),
            cost_by_model: merged_by_label(&self.cost_by_model, &other.cost_by_model, true),
            cost_by_agent: merged_by_label(&self.cost_by_agent, &other.cost_by_agent, true),
            cost_by_harness: merged_by_label(&self.cost_by_harness, &other.cost_by_harness, true),
            sessions_by_day: merged_by_label(&self.sessions_by_day, &other.sessions_by_day, false),
            sessions_by_agent: merged_by_label(
                &self.sessions_by_agent,
                &other.sessions_by_agent,
                true,
            ),
            sessions_by_model: merged_by_label(
                &self.sessions_by_model,
                &other.sessions_by_model,
                true,
            ),
            tokens_by_day: merged_by_label(&self.tokens_by_day, &other.tokens_by_day, false),
            tool_usage: merged_by_label(&self.tool_usage, &other.tool_usage, true),
            tool_time: merged_by_label(&self.tool_time, &other.tool_time, true),
            activity_by_day: merged_activity(&self.activity_by_day, &other.activity_by_day),
            sessions,
        }
    }

    /// Grouped series keep their `top` by value, day series keep their newest
    /// `top` days, and the session list keeps its newest `top` rows.
    pub fn truncated(mut self, top: i64) -> Self {
        let top = usize::try_from(top.max(1)).unwrap_or(1);
        self.cost_by_project.truncate(top);
        self.cost_by_model.truncate(top);
        self.cost_by_agent.truncate(top);
        self.cost_by_harness.truncate(top);
        self.sessions_by_agent.truncate(top);
        self.sessions_by_model.truncate(top);
        self.tool_usage.truncate(top);
        self.tool_time.truncate(top);
        self.cost_by_day = keep_last(&self.cost_by_day, top);
        self.sessions_by_day = keep_last(&self.sessions_by_day, top);
        self.tokens_by_day = keep_last(&self.tokens_by_day, top);
        self.activity_by_day = keep_last(&self.activity_by_day, top);
        self.sessions.truncate(top);
        self
    }

    /// Labels a single harness's report as its own: the summary a view
    /// compares harnesses with, and that harness's cost point. A blank
    /// report stays blank so the merge skips it.
    pub fn attributed(mut self, label: &str) -> Self {
        if !self.is_blank() {
            self.harnesses.push(MiningHarness {
                label: label.into(),
                sessions: self.totals.sessions,
                messages: self.totals.messages,
                tool_calls: self.totals.tool_calls,
                patches: self.totals.patches,
                cost_usd: self.totals.cost_usd,
                tokens_input: self.totals.tokens_input,
                tokens_output: self.totals.tokens_output,
                tokens_cache_read: self.totals.tokens_cache_read,
            });
            self.cost_by_harness.push(MiningSeriesPoint {
                label: label.into(),
                value: Value::from(self.totals.cost_usd),
            });
        }
        self
    }
}

fn add_harness(base: &mut MiningHarness, addend: &MiningHarness) {
    base.sessions += addend.sessions;
    base.messages += addend.messages;
    base.tool_calls += addend.tool_calls;
    base.patches += addend.patches;
    base.cost_usd += addend.cost_usd;
    base.tokens_input += addend.tokens_input;
    base.tokens_output += addend.tokens_output;
    base.tokens_cache_read += addend.tokens_cache_read;
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn summed(left: &Value, right: &Value) -> Value {
    Value::from(number(left) + number(right))
}

fn merged_by_label(
    left: &[MiningSeriesPoint],
    right: &[MiningSeriesPoint],
    by_value: bool,
) -> Vec<MiningSeriesPoint> {
    let mut points = left.to_vec();
    for point in right {
        match points
            .iter_mut()
            .find(|existing| existing.label == point.label)
        {
            Some(existing) => existing.value = summed(&existing.value, &point.value),
            None => points.push(point.clone()),
        }
    }
    if by_value {
        points.sort_by(|a, b| number(&b.value).total_cmp(&number(&a.value)));
    } else {
        points.sort_by(|a, b| a.label.cmp(&b.label));
    }
    points
}

fn merged_activity(
    left: &[MiningActivityDay],
    right: &[MiningActivityDay],
) -> Vec<MiningActivityDay> {
    let mut days: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for source in [left, right] {
        for day in source {
            let values = days
                .entry(day.label.clone())
                .or_insert_with(|| vec![0; day.values.len()]);
            for (slot, value) in day.values.iter().enumerate() {
                if let Some(target) = values.get_mut(slot) {
                    *target += *value;
                }
            }
        }
    }
    days.into_iter()
        .map(|(label, values)| MiningActivityDay { label, values })
        .collect()
}

fn keep_last<T: Clone>(entries: &[T], top: usize) -> Vec<T> {
    let start = entries.len().saturating_sub(top);
    entries[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn near(left: f64, right: f64) -> bool {
        (left - right).abs() < f64::EPSILON
    }

    fn point(label: &str, value: Value) -> MiningSeriesPoint {
        MiningSeriesPoint {
            label: label.into(),
            value,
        }
    }

    fn session(id: &str, updated: u64) -> MinedSession {
        MinedSession {
            id: id.into(),
            harness: "opencode".into(),
            title: id.into(),
            directory: "/w".into(),
            cost_usd: 1.0,
            tokens_input: 0,
            tokens_output: 0,
            tokens_reasoning: 0,
            tokens_cache_read: 0,
            model: None,
            agent: None,
            time_created: updated,
            time_updated: updated,
        }
    }

    fn harness(label: &str, sessions: u64, cost: f64) -> MiningHarness {
        MiningHarness {
            label: label.into(),
            sessions,
            messages: sessions,
            tool_calls: 0,
            patches: 0,
            cost_usd: cost,
            tokens_input: 0,
            tokens_output: 0,
            tokens_cache_read: 0,
        }
    }

    #[test]
    fn a_blank_side_leaves_the_other_report_untouched() {
        let report = MiningReport {
            totals: MiningTotals {
                sessions: 2,
                ..MiningTotals::default()
            },
            cost_by_project: vec![point("mitos", json!(3.5))],
            ..MiningReport::default()
        };
        let merged = report.clone().merged(MiningReport::default());
        assert_eq!(merged, report);
    }

    /// Two harness reports whose labels overlap and differ, so every merge
    /// path has something to add.
    fn merge_sides() -> (MiningReport, MiningReport) {
        let left = MiningReport {
            totals: MiningTotals {
                sessions: 1,
                messages_user: 2,
                cost_usd: 2.0,
                tool_seconds: 1.5,
                tokens_input: 100,
                ..MiningTotals::default()
            },
            harnesses: vec![harness("opencode", 1, 2.0)],
            cost_by_day: vec![
                point("2026-01-01", json!(1.0)),
                point("2026-01-02", json!(2.0)),
            ],
            cost_by_project: vec![point("mitos", json!(2.0)), point("nix", json!(1.0))],
            cost_by_agent: vec![point("build", json!(2.0))],
            cost_by_harness: vec![point("opencode", json!(2.0))],
            sessions_by_day: vec![point("2026-01-02", json!(1))],
            tool_usage: vec![point("bash", json!(3))],
            tool_time: vec![point("bash", json!(1.5))],
            activity_by_day: vec![MiningActivityDay {
                label: "2026-01-01".into(),
                values: vec![1, 2],
            }],
            sessions: vec![session("a", 20)],
            ..MiningReport::default()
        };
        let right = MiningReport {
            totals: MiningTotals {
                sessions: 2,
                messages_user: 3,
                cost_usd: 3.0,
                tool_seconds: 2.5,
                tokens_input: 50,
                ..MiningTotals::default()
            },
            harnesses: vec![harness("opencode", 1, 1.0), harness("claude", 1, 2.0)],
            cost_by_day: vec![
                point("2026-01-02", json!(5.0)),
                point("2026-01-03", json!(1.0)),
            ],
            cost_by_project: vec![point("nix", json!(4.0))],
            cost_by_agent: vec![point("build", json!(1.0)), point("plan", json!(0.5))],
            cost_by_harness: vec![point("claude", json!(2.0))],
            sessions_by_day: vec![point("2026-01-02", json!(2))],
            tool_usage: vec![point("bash", json!(2)), point("read", json!(9))],
            tool_time: vec![point("bash", json!(2.5))],
            activity_by_day: vec![MiningActivityDay {
                label: "2026-01-01".into(),
                values: vec![0, 7],
            }],
            sessions: vec![session("b", 30), session("c", 10)],
            ..MiningReport::default()
        };
        (left, right)
    }

    #[test]
    fn merged_reports_add_totals_and_harness_figures() {
        let (left, right) = merge_sides();
        let merged = left.merged(right);

        assert_eq!(merged.totals.sessions, 3);
        assert!(near(merged.totals.cost_usd, 5.0));
        assert!(near(merged.totals.tool_seconds, 4.0));
        assert_eq!(merged.totals.messages_user, 5);
        assert_eq!(merged.totals.tokens_input, 150);
        assert_eq!(
            merged.harnesses,
            vec![harness("opencode", 2, 3.0), harness("claude", 1, 2.0),]
        );
        assert_eq!(
            merged.cost_by_harness,
            vec![point("opencode", json!(2.0)), point("claude", json!(2.0))]
        );
    }

    #[test]
    fn merged_reports_add_labels_and_rank_grouped_series_by_value() {
        let (left, right) = merge_sides();
        let merged = left.merged(right);

        assert_eq!(
            merged.cost_by_agent,
            vec![point("build", json!(3.0)), point("plan", json!(0.5))]
        );
        assert_eq!(
            merged.sessions_by_day,
            vec![point("2026-01-02", json!(3.0))]
        );
        assert_eq!(merged.tool_time, vec![point("bash", json!(4.0))]);
        assert_eq!(
            merged.cost_by_day,
            vec![
                point("2026-01-01", json!(1.0)),
                point("2026-01-02", json!(7.0)),
                point("2026-01-03", json!(1.0)),
            ]
        );
        assert_eq!(
            merged.cost_by_project,
            vec![point("nix", json!(5.0)), point("mitos", json!(2.0))]
        );
        assert_eq!(
            merged.tool_usage,
            vec![point("read", json!(9)), point("bash", json!(5.0))]
        );
        assert_eq!(
            merged.activity_by_day,
            vec![MiningActivityDay {
                label: "2026-01-01".into(),
                values: vec![1, 9],
            }]
        );
        assert_eq!(
            merged
                .sessions
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
    }

    #[test]
    fn truncated_keeps_top_grouped_series_and_newest_days() {
        let report = MiningReport {
            cost_by_project: vec![
                point("a", json!(3.0)),
                point("b", json!(2.0)),
                point("c", json!(1.0)),
            ],
            cost_by_harness: vec![point("opencode", json!(4.0)), point("claude", json!(1.0))],
            tool_time: vec![
                point("bash", json!(9.0)),
                point("read", json!(1.0)),
                point("grep", json!(0.5)),
            ],
            cost_by_day: vec![
                point("2026-01-01", json!(1.0)),
                point("2026-01-02", json!(1.0)),
                point("2026-01-03", json!(1.0)),
            ],
            sessions: vec![session("a", 30), session("b", 20), session("c", 10)],
            ..MiningReport::default()
        };

        let truncated = report.truncated(2);

        assert_eq!(
            truncated.cost_by_project,
            vec![point("a", json!(3.0)), point("b", json!(2.0))]
        );
        assert_eq!(
            truncated.cost_by_harness,
            vec![point("opencode", json!(4.0)), point("claude", json!(1.0))]
        );
        assert_eq!(
            truncated.tool_time,
            vec![point("bash", json!(9.0)), point("read", json!(1.0))]
        );
        assert_eq!(
            truncated.cost_by_day,
            vec![
                point("2026-01-02", json!(1.0)),
                point("2026-01-03", json!(1.0))
            ]
        );
        assert_eq!(
            truncated
                .sessions
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
