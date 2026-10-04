use serde::{Deserialize, Serialize};

use super::{HarnessKind, ThreadId, Timestamp, TurnId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub id: String,
    pub thread_id: ThreadId,
    pub harness: HarnessKind,
    pub turn_id: Option<TurnId>,
    pub observed_at: Timestamp,
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
    pub harness: HarnessKind,
    pub observed_at: Timestamp,
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
