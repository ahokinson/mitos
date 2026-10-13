//! The accumulator JSONL harness miners feed, and the ranked series it
//! builds. One harness's sessions fold into maps first so labels merge
//! without borrow gymnastics; `finish` turns each map into its series.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::json::string_value;
use crate::wire::responses::{
    MinedSession, MiningActivityDay, MiningReport, MiningSeriesPoint, MiningTotals,
};

pub const HOURS_PER_DAY: usize = 24;
/// The provider ceiling; the merged report is trimmed to the request's `top`.
pub const SESSIONS_CEILING: usize = 1000;
const TITLE_MAX: usize = 80;

#[derive(Default)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl TokenCounts {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.reasoning + self.cache_read + self.cache_write
    }
}

/// One mined session file's figures, ready to fold into a report.
pub struct MinedStats {
    pub id: String,
    pub title: Option<String>,
    pub directory: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub day: String,
    pub cost_usd: f64,
    pub tool_seconds: f64,
    pub messages_user: u64,
    pub messages_assistant: u64,
    pub tool_calls: u64,
    pub tools: Vec<(String, u64)>,
    pub model_costs: Vec<(String, f64)>,
    pub tokens: TokenCounts,
    pub activity: Vec<(String, [u64; HOURS_PER_DAY])>,
    pub time_created: u64,
    pub time_updated: u64,
}

pub struct ReportBuilder {
    harness: String,
    totals: MiningTotals,
    sessions: Vec<MinedSession>,
    day_costs: BTreeMap<String, f64>,
    day_tokens: BTreeMap<String, u64>,
    day_sessions: BTreeMap<String, u64>,
    project_costs: BTreeMap<String, f64>,
    model_costs: BTreeMap<String, f64>,
    agent_costs: BTreeMap<String, f64>,
    agent_sessions: BTreeMap<String, u64>,
    model_sessions: BTreeMap<String, u64>,
    tool_usage: BTreeMap<String, u64>,
    activity: BTreeMap<String, [u64; HOURS_PER_DAY]>,
}

impl ReportBuilder {
    pub fn new(harness: &str) -> Self {
        Self {
            harness: harness.into(),
            totals: MiningTotals::default(),
            sessions: Vec::new(),
            day_costs: BTreeMap::new(),
            day_tokens: BTreeMap::new(),
            day_sessions: BTreeMap::new(),
            project_costs: BTreeMap::new(),
            model_costs: BTreeMap::new(),
            agent_costs: BTreeMap::new(),
            agent_sessions: BTreeMap::new(),
            model_sessions: BTreeMap::new(),
            tool_usage: BTreeMap::new(),
            activity: BTreeMap::new(),
        }
    }

    pub fn absorb(&mut self, stats: MinedStats) {
        self.totals.sessions += 1;
        self.totals.messages += stats.messages_user + stats.messages_assistant;
        self.totals.messages_user += stats.messages_user;
        self.totals.messages_assistant += stats.messages_assistant;
        self.totals.tool_calls += stats.tool_calls;
        self.totals.cost_usd += stats.cost_usd;
        self.totals.tool_seconds += stats.tool_seconds;
        self.totals.tokens_input += stats.tokens.input;
        self.totals.tokens_output += stats.tokens.output;
        self.totals.tokens_reasoning += stats.tokens.reasoning;
        self.totals.tokens_cache_read += stats.tokens.cache_read;
        self.totals.tokens_cache_write += stats.tokens.cache_write;

        *self.day_costs.entry(stats.day.clone()).or_default() += stats.cost_usd;
        *self.day_tokens.entry(stats.day.clone()).or_default() += stats.tokens.total();
        *self.day_sessions.entry(stats.day).or_default() += 1;
        *self
            .project_costs
            .entry(stats.directory.clone())
            .or_default() += stats.cost_usd;
        for (model, cost) in stats.model_costs {
            *self.model_costs.entry(model).or_default() += cost;
        }
        if let Some(model) = &stats.model {
            *self.model_sessions.entry(model.clone()).or_default() += 1;
        }
        if let Some(agent) = &stats.agent {
            *self.agent_sessions.entry(agent.clone()).or_default() += 1;
            *self.agent_costs.entry(agent.clone()).or_default() += stats.cost_usd;
        }
        for (name, calls) in stats.tools {
            *self.tool_usage.entry(name).or_default() += calls;
        }
        for (day, values) in stats.activity {
            let slot = self.activity.entry(day).or_default();
            for (hour, count) in values.iter().enumerate() {
                if let Some(target) = slot.get_mut(hour) {
                    *target += count;
                }
            }
        }

        self.sessions.push(MinedSession {
            id: stats.id,
            harness: self.harness.clone(),
            title: stats
                .title
                .unwrap_or_default()
                .chars()
                .take(TITLE_MAX)
                .collect(),
            directory: stats.directory,
            cost_usd: stats.cost_usd,
            tokens_input: stats.tokens.input,
            tokens_output: stats.tokens.output,
            tokens_reasoning: stats.tokens.reasoning,
            tokens_cache_read: stats.tokens.cache_read,
            model: stats.model,
            agent: stats.agent,
            time_created: stats.time_created,
            time_updated: stats.time_updated,
        });
    }

    /// The day series keep label order, which is date order; the grouped
    /// series rank by value. `finish` never attributes — the caller names
    /// the harness so a blank pass stays blank.
    pub fn finish(self) -> MiningReport {
        MiningReport {
            totals: self.totals,
            harnesses: Vec::new(),
            est_windows: None,
            cost_by_day: ranked_f64(&self.day_costs, false),
            cost_by_project: ranked_f64(&self.project_costs, true),
            cost_by_model: ranked_f64(&self.model_costs, true),
            cost_by_agent: ranked_f64(&self.agent_costs, true),
            cost_by_harness: Vec::new(),
            sessions_by_day: ranked_u64(&self.day_sessions, false),
            sessions_by_agent: ranked_u64(&self.agent_sessions, true),
            sessions_by_model: ranked_u64(&self.model_sessions, true),
            tokens_by_day: ranked_u64(&self.day_tokens, false),
            tool_usage: ranked_u64(&self.tool_usage, true),
            tool_time: Vec::new(),
            activity_by_day: self
                .activity
                .into_iter()
                .map(|(label, values)| MiningActivityDay {
                    label,
                    values: values.to_vec(),
                })
                .collect(),
            sessions: {
                let mut sessions = self.sessions;
                sessions.sort_by_key(|session| std::cmp::Reverse(session.time_updated));
                sessions
            },
        }
    }
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn ranked_f64(map: &BTreeMap<String, f64>, by_value: bool) -> Vec<MiningSeriesPoint> {
    let mut points: Vec<MiningSeriesPoint> = map
        .iter()
        .map(|(label, value)| MiningSeriesPoint {
            label: label.clone(),
            value: Value::from(*value),
        })
        .collect();
    if by_value {
        points.sort_by(|a, b| num(&b.value).total_cmp(&num(&a.value)));
    }
    points
}

fn ranked_u64(map: &BTreeMap<String, u64>, by_value: bool) -> Vec<MiningSeriesPoint> {
    let mut points: Vec<MiningSeriesPoint> = map
        .iter()
        .map(|(label, value)| MiningSeriesPoint {
            label: label.clone(),
            value: Value::from(*value),
        })
        .collect();
    if by_value {
        points.sort_by(|a, b| num(&b.value).total_cmp(&num(&a.value)));
    }
    points
}

/// An ISO-8601 timestamp as milliseconds since the epoch.
pub fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
    string_value(value)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|parsed| parsed.with_timezone(&chrono::Utc).timestamp_millis())
}

/// `2026-10-10` from epoch milliseconds.
pub fn day_label(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// The UTC hour, 0-23, from epoch milliseconds.
pub fn hour_slot(ms: i64) -> Option<usize> {
    chrono::DateTime::from_timestamp_millis(ms)
        .and_then(|date| date.format("%H").to_string().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(id: &str, day: &str, cost: f64) -> MinedStats {
        MinedStats {
            id: id.into(),
            title: Some(format!("title of {id}")),
            directory: "/w".into(),
            agent: Some("build".into()),
            model: Some("glm-5.3".into()),
            day: day.into(),
            cost_usd: cost,
            tool_seconds: 0.0,
            messages_user: 1,
            messages_assistant: 2,
            tool_calls: 3,
            tools: vec![("bash".into(), 3)],
            model_costs: vec![("glm-5.3".into(), cost)],
            tokens: TokenCounts {
                input: 10,
                output: 5,
                ..TokenCounts::default()
            },
            activity: vec![(day.into(), {
                let mut values = [0; HOURS_PER_DAY];
                values[3] = 3;
                values
            })],
            time_created: 1000,
            time_updated: 2000,
        }
    }

    #[test]
    fn hour_slots_read_the_utc_hour_and_none_when_out_of_range() {
        assert_eq!(hour_slot(0), Some(0));
        assert_eq!(hour_slot(5 * 3_600_000), Some(5));
        assert_eq!(hour_slot(23 * 3_600_000), Some(23));
        assert_eq!(hour_slot(i64::MAX), None);
    }

    #[test]
    fn absorbed_sessions_fold_into_totals_series_and_days() {
        let mut builder = ReportBuilder::new("claude");
        builder.absorb(stats("a", "2026-01-02", 2.0));
        builder.absorb(stats("b", "2026-01-02", 1.0));
        builder.absorb(stats("c", "2026-01-03", 4.0));
        let report = builder.finish().attributed("claude");

        assert_eq!(report.totals.sessions, 3);
        assert_eq!(report.totals.messages, 9);
        assert_eq!(report.totals.messages_user, 3);
        assert_eq!(report.totals.messages_assistant, 6);
        assert_eq!(report.totals.tool_calls, 9);
        assert_eq!(report.totals.tokens_input, 30);
        assert_eq!(
            report.cost_by_day,
            vec![
                MiningSeriesPoint {
                    label: "2026-01-02".into(),
                    value: Value::from(3.0)
                },
                MiningSeriesPoint {
                    label: "2026-01-03".into(),
                    value: Value::from(4.0)
                },
            ]
        );
        assert_eq!(
            report.sessions_by_day,
            vec![
                MiningSeriesPoint {
                    label: "2026-01-02".into(),
                    value: Value::from(2_u64)
                },
                MiningSeriesPoint {
                    label: "2026-01-03".into(),
                    value: Value::from(1_u64)
                },
            ]
        );
        assert_eq!(
            report.cost_by_project,
            vec![MiningSeriesPoint {
                label: "/w".into(),
                value: Value::from(7.0)
            }]
        );
        assert_eq!(
            report.tool_usage,
            vec![MiningSeriesPoint {
                label: "bash".into(),
                value: Value::from(9_u64)
            }]
        );
        assert_eq!(report.sessions[0].harness, "claude");
        assert_eq!(report.sessions[0].title, "title of a");
        assert_eq!(report.harnesses.len(), 1);
        assert_eq!(report.harnesses[0].label, "claude");
        assert_eq!(report.harnesses[0].sessions, 3);
    }
}
