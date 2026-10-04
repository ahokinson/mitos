use serde::Serialize;

use super::{HarnessKind, Timestamp};

/// What one harness hook invocation reported. Cost is the harness's own
/// running total for the native session.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Observation {
    pub harness: HarnessKind,
    pub event: Option<String>,
    pub native_session: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
    pub context_used_tokens: Option<u64>,
    pub context_limit_tokens: Option<u64>,
    pub plan: Option<PlanWindows>,
}

impl Observation {
    pub fn new(harness: HarnessKind) -> Self {
        Self {
            harness,
            event: None,
            native_session: None,
            cwd: None,
            model: None,
            cost_usd: None,
            context_used_tokens: None,
            context_limit_tokens: None,
            plan: None,
        }
    }
}

/// Account-wide rate-limit windows; each is independently absent.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PlanWindows {
    pub five_hour_percent: Option<f64>,
    pub five_hour_resets_at: Option<String>,
    pub week_percent: Option<f64>,
    pub week_resets_at: Option<String>,
}

/// The last time a harness's hook reported anything.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HookLastSeen {
    pub harness: HarnessKind,
    pub observed_at: Timestamp,
}
