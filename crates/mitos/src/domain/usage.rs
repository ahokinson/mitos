use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub id: String,
    pub thread_id: String,
    pub harness: String,
    pub turn_id: Option<String>,
    pub observed_at: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub context_used_tokens: Option<u64>,
    pub context_limit_tokens: Option<u64>,
    pub model: Option<String>,
    pub turns: Option<u32>,
}

/// What a frontend shows for a thread: the latest context and model readings,
/// cumulative tokens and cost across the whole thread, and the harness's
/// account-wide rate windows.
#[derive(Clone, Debug, Serialize)]
pub struct ThreadUsage {
    pub harness: String,
    pub observed_at: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub context_used_tokens: Option<u64>,
    pub context_limit_tokens: Option<u64>,
    pub model: Option<String>,
    pub turns: Option<u32>,
    pub plan_five_hour_percent: Option<f64>,
    pub plan_five_hour_resets_at: Option<String>,
    pub plan_week_percent: Option<f64>,
    pub plan_week_resets_at: Option<String>,
}

/// Account-wide, not thread-scoped.
#[cfg(test)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarnessPlanUsage {
    pub harness: String,
    pub plan_five_hour_percent: Option<f64>,
    pub plan_five_hour_resets_at: Option<String>,
    pub plan_week_percent: Option<f64>,
    pub plan_week_resets_at: Option<String>,
    pub observed_at: String,
}
