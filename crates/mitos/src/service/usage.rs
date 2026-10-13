use crate::config::{Plan, plan_for};
use crate::domain::HarnessKind;
use crate::store::{PlanUsageFields, UsageFields};
use crate::wire::responses::CollectedUsage;

pub(super) fn usage_fields(
    usage: &CollectedUsage,
    plans: &[Plan],
    harness: HarnessKind,
) -> UsageFields {
    let plan = plan_for(plans, harness, usage.provider.as_deref());
    UsageFields {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cost_usd: usage.cost_usd,
        context_used_tokens: usage.context_used_tokens,
        context_limit_tokens: usage.context_limit_tokens,
        model: usage.model.clone(),
        provider: usage.provider.clone(),
        plan: plan.map(|plan| plan.id().into()),
        turns: usage.turns,
    }
}

pub(super) fn plan_usage_fields(usage: &CollectedUsage) -> Option<PlanUsageFields> {
    (usage.plan_five_hour_percent.is_some()
        || usage.plan_week_percent.is_some()
        || usage.plan_month_percent.is_some())
    .then(|| PlanUsageFields {
        plan_five_hour_percent: usage.plan_five_hour_percent,
        plan_five_hour_resets_at: usage.plan_five_hour_resets_at.clone(),
        plan_week_percent: usage.plan_week_percent,
        plan_week_resets_at: usage.plan_week_resets_at.clone(),
        plan_month_percent: usage.plan_month_percent,
        plan_month_resets_at: usage.plan_month_resets_at.clone(),
    })
}
