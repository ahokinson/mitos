use crate::store::{PlanUsageFields, UsageFields};
use crate::wire::responses::CollectedUsage;

pub(super) fn usage_fields(usage: &CollectedUsage) -> UsageFields {
    UsageFields {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cost_usd: usage.cost_usd,
        context_used_tokens: usage.context_used_tokens,
        context_limit_tokens: usage.context_limit_tokens,
        model: usage.model.clone(),
        turns: usage.turns,
    }
}

pub(super) fn plan_usage_fields(usage: &CollectedUsage) -> Option<PlanUsageFields> {
    (usage.plan_five_hour_percent.is_some() || usage.plan_week_percent.is_some()).then(|| {
        PlanUsageFields {
            plan_five_hour_percent: usage.plan_five_hour_percent,
            plan_five_hour_resets_at: usage.plan_five_hour_resets_at.clone(),
            plan_week_percent: usage.plan_week_percent,
            plan_week_resets_at: usage.plan_week_resets_at.clone(),
        }
    })
}
