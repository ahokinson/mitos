use chrono::{Datelike, Timelike};
use serde_json::Value;

use crate::config::Plan;
use crate::history::ReadonlyDatabase;
use crate::json::{number_value, string_value};
use crate::wire::responses::EstWindows;

const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_DAY: i64 = 86_400_000;
const ROLLING_WINDOW_MS: i64 = 5 * MS_PER_HOUR;
const WEEK_MS: i64 = 7 * MS_PER_DAY;

/// Monthly visible-spend allowances per Go model, from opencode.ai/docs/go:
/// (model id, Go tier, Go Plus tier). Free preview models never burn against
/// the windows; a model missing from the table gets the tier's most generous
/// allowance so an estimate never overstates the burn.
const MODEL_ALLOWANCES: [(&str, f64, f64); 30] = [
    ("glm-5.3-flash", 60.0, 180.0),
    ("glm-5.3", 15.0, 120.0),
    ("glm-5.2", 60.0, 180.0),
    ("kimi-k3", 15.0, 60.0),
    ("kimi-k2.7-code", 60.0, 180.0),
    ("kimi-k2.6", 60.0, 240.0),
    ("longcat-2.0", 60.0, 240.0),
    ("mimo-v2.6-flash", 60.0, 120.0),
    ("mimo-v2.6-pro", 15.0, 60.0),
    ("mimo-v2.5", 60.0, 120.0),
    ("mimo-v2.5-pro", 15.0, 60.0),
    ("minimax-m3", 60.0, 180.0),
    ("minimax-m2.7", 60.0, 240.0),
    ("muse-spark-1.3-contributor", 60.0, 120.0),
    ("muse-spark-1.2-contributor", 60.0, 120.0),
    ("qwen3.8-max", 15.0, 60.0),
    ("qwen3.8-flash", 30.0, 90.0),
    ("qwen3.7-plus", 60.0, 180.0),
    ("deepseek-v4.1-flash", 60.0, 120.0),
    ("deepseek-v4-pro", 15.0, 60.0),
    ("deepseek-v4-flash", 30.0, 120.0),
    ("deepseek-v4-flash-vision-exp", 15.0, 60.0),
    ("hy4-preview", 30.0, 120.0),
    ("hy3", 60.0, 240.0),
    ("space-bunny", 30.0, 120.0),
    ("grok-4.7", 15.0, 60.0),
    ("grok-4.6", 15.0, 60.0),
    ("gpt-6-luna", 15.0, 60.0),
    ("gpt-5.6-luna", 15.0, 60.0),
    ("claude-haiku-5-5", 15.0, 60.0),
];

const FREE_MODELS: [&str; 2] = ["longcat-2.5-preview-free", "step-5-preview-free"];

/// `None` for a model that never burns against the windows.
fn monthly_allowance(model: &str, tier: &str) -> Option<f64> {
    if FREE_MODELS.contains(&model) {
        return None;
    }
    let is_plus = tier == "plus";
    let fallback = if is_plus { 240.0 } else { 60.0 };
    Some(
        MODEL_ALLOWANCES
            .iter()
            .find(|(id, _, _)| *id == model)
            .map_or(fallback, |&(_, go, plus)| if is_plus { plus } else { go }),
    )
}

/// One machine's share of the Go plan windows, estimated from the per-request
/// costs the harness already recorded: each request's visible cost over its
/// model's monthly allowance, summed per window as a fraction of that
/// allowance. Plan windows are account-wide, so no directory filter applies
/// and other machines' spend is simply missing, not zero.
pub(super) fn est_windows(database: &ReadonlyDatabase, plan: Plan, now: i64) -> Option<EstWindows> {
    let tier = plan.go_tier_label()?;
    let rows = database
        .query(
            "SELECT p.time_created AS at, json_extract(p.data, '$.cost') AS cost, \
              json_extract(m.data, '$.model.id') AS model, \
              json_extract(m.data, '$.model.providerID') AS provider \
             FROM part p JOIN message m ON p.message_id = m.id \
             WHERE json_extract(p.data, '$.type') = 'step-finish' \
             AND json_extract(p.data, '$.cost') IS NOT NULL",
            (),
        )
        .ok()?;
    let month_start = month_start_ms(now);
    let mut burn_5h = 0.0;
    let mut oldest_5h: Option<i64> = None;
    let mut burn_week = 0.0;
    let mut burn_month = 0.0;
    let mut burnable = false;
    for row in rows {
        let Some(at) = row.get("at").and_then(Value::as_i64) else {
            continue;
        };
        let Some(cost) = number_value(row.get("cost")) else {
            continue;
        };
        let model = string_value(row.get("model")).unwrap_or_default();
        if string_value(row.get("provider")) != Some("opencode-go") {
            continue;
        }
        let Some(allowance) = monthly_allowance(model, tier) else {
            continue;
        };
        burnable = true;
        let burn = cost / allowance;
        if at > now - ROLLING_WINDOW_MS {
            burn_5h += burn;
            oldest_5h = Some(oldest_5h.map_or(at, |oldest: i64| oldest.min(at)));
        }
        if at > now - WEEK_MS {
            burn_week += burn;
        }
        if at >= month_start {
            burn_month += burn;
        }
    }
    burnable.then(|| EstWindows {
        tier: tier.into(),
        burn_5h,
        oldest_5h_ms: oldest_5h,
        burn_week,
        burn_month,
    })
}

/// The UTC calendar month's first millisecond; the console anchors the month
/// to the subscription date, so this estimate approximates that boundary.
fn month_start_ms(now: i64) -> i64 {
    chrono::DateTime::from_timestamp_millis(now).map_or(now, |date| {
        let into_day = (i64::from(date.hour()) * 3_600
            + i64::from(date.minute()) * 60
            + i64::from(date.second()))
            * 1000
            + i64::from(date.timestamp_subsec_millis());
        now - (i64::from(date.day()) - 1) * MS_PER_DAY - into_day
    })
}
