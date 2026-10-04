use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use crate::domain::{HarnessKind, Observation, PlanWindows};

/// `None` for a payload that is not a JSON object.
pub fn parse(harness: HarnessKind, event: Option<&str>, payload: &Value) -> Option<Observation> {
    if !payload.is_object() {
        return None;
    }
    let mut observation = match harness {
        HarnessKind::Claude => claude(payload),
        HarnessKind::Codex => codex(payload),
        HarnessKind::Hermes => hermes(payload),
        HarnessKind::OpenCode => opencode(payload),
    };
    observation.event = event
        .map(str::to_owned)
        .or(observation.event)
        .filter(|event| !event.is_empty());
    Some(observation)
}

/// The statusLine payload: running session cost, live context window and
/// account rate limits.
fn claude(payload: &Value) -> Observation {
    let model = payload.get("model");
    let size = u64_at(payload, &["context_window", "context_window_size"]);
    Observation {
        event: Some("statusline".into()),
        native_session: str_at(payload, &["session_id"]),
        cwd: str_at(payload, &["cwd"]).or_else(|| str_at(payload, &["workspace", "current_dir"])),
        model: model
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| str_at(payload, &["model", "id"]))
            .or_else(|| str_at(payload, &["model", "display_name"])),
        cost_usd: f64_at(payload, &["cost", "total_cost_usd"]),
        context_used_tokens: context_used(payload, size),
        context_limit_tokens: size,
        plan: Some(PlanWindows {
            five_hour_percent: fresh_window(payload, "five_hour").map(|window| window.0),
            five_hour_resets_at: fresh_window(payload, "five_hour").map(|window| window.1),
            week_percent: fresh_window(payload, "seven_day").map(|window| window.0),
            week_resets_at: fresh_window(payload, "seven_day").map(|window| window.1),
        }),
        ..Observation::new(HarnessKind::Claude)
    }
}

/// Context occupancy: the last call's input side, else the percentage of the
/// window. Claude computes its percentage from exactly that input side.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn context_used(payload: &Value, size: Option<u64>) -> Option<u64> {
    let sum = value_at(payload, &["context_window", "current_usage"]).and_then(|usage| {
        [
            "input_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ]
        .iter()
        .filter_map(|key| usage.get(key).and_then(Value::as_u64))
        .reduce(|total, tokens| total + tokens)
    });
    if sum.is_some() {
        return sum;
    }
    let percent = f64_at(payload, &["context_window", "used_percentage"])?;
    size.map(|size| (percent / 100.0 * size as f64).round() as u64)
}

/// A window whose reset time has passed no longer means anything.
fn fresh_window(payload: &Value, name: &str) -> Option<(f64, String)> {
    let window = payload.get("rate_limits")?.get(name)?;
    let percent = window.get("used_percentage")?.as_f64()?;
    let resets_at = DateTime::<Utc>::from_timestamp(window.get("resets_at")?.as_i64()?, 0)?;
    (resets_at > Utc::now()).then(|| {
        (
            percent,
            resets_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        )
    })
}

fn codex(payload: &Value) -> Observation {
    Observation {
        event: str_at(payload, &["hook_event_name"]),
        native_session: str_at(payload, &["session_id"]),
        cwd: str_at(payload, &["cwd"]),
        model: str_at(payload, &["model"]),
        ..Observation::new(HarnessKind::Codex)
    }
}

fn hermes(payload: &Value) -> Observation {
    Observation {
        event: str_at(payload, &["hook_event_name"]),
        native_session: str_at(payload, &["session_id"]),
        cwd: str_at(payload, &["cwd"]),
        model: str_at(payload, &["extra", "model"]),
        ..Observation::new(HarnessKind::Hermes)
    }
}

/// The envelope the installed plugin forwards: `{type, properties}`. Only
/// `session.*` events carry the session record with its running cost.
fn opencode(payload: &Value) -> Observation {
    let properties = payload.get("properties");
    let info = properties.and_then(|properties| properties.get("info"));
    let in_info = |keys: &[&str]| info.and_then(|info| value_at(info, keys));
    Observation {
        event: str_at(payload, &["type"]),
        native_session: in_info(&["id"])
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| properties.and_then(|p| str_at(p, &["sessionID"]))),
        cwd: in_info(&["directory"])
            .and_then(Value::as_str)
            .map(str::to_owned),
        model: in_info(&["model", "id"])
            .or_else(|| in_info(&["model"]))
            .and_then(Value::as_str)
            .map(str::to_owned),
        cost_usd: in_info(&["cost"]).and_then(Value::as_f64),
        ..Observation::new(HarnessKind::OpenCode)
    }
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |node, key| node.get(key))
}

fn str_at(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn f64_at(value: &Value, path: &[&str]) -> Option<f64> {
    value_at(value, path).and_then(Value::as_f64)
}

fn u64_at(value: &Value, path: &[&str]) -> Option<u64> {
    value_at(value, path).and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse;
    use crate::domain::HarnessKind;

    fn future(seconds: i64) -> i64 {
        chrono::Utc::now().timestamp() + seconds
    }

    #[test]
    fn claude_statusline_carries_cost_context_and_limits() {
        let payload = json!({
            "session_id": "abc",
            "cwd": "/work",
            "model": { "id": "claude-opus-5", "display_name": "Opus" },
            "cost": { "total_cost_usd": 0.01234 },
            "context_window": {
                "context_window_size": 200_000,
                "used_percentage": 8,
                "current_usage": {
                    "input_tokens": 100,
                    "cache_creation_input_tokens": 400,
                    "cache_read_input_tokens": 15_500
                }
            },
            "rate_limits": {
                "five_hour": { "used_percentage": 23.5, "resets_at": future(3600) },
                "seven_day": { "used_percentage": 41.2, "resets_at": future(86_400) }
            }
        });
        let observation = parse(HarnessKind::Claude, None, &payload).unwrap();

        assert_eq!(observation.harness, HarnessKind::Claude);
        assert_eq!(observation.event.as_deref(), Some("statusline"));
        assert_eq!(observation.native_session.as_deref(), Some("abc"));
        assert_eq!(observation.cwd.as_deref(), Some("/work"));
        assert_eq!(observation.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(observation.cost_usd, Some(0.01234));
        assert_eq!(observation.context_used_tokens, Some(16_000));
        assert_eq!(observation.context_limit_tokens, Some(200_000));
        let plan = observation.plan.unwrap();
        assert_eq!(plan.five_hour_percent, Some(23.5));
        assert_eq!(plan.week_percent, Some(41.2));
        assert!(plan.five_hour_resets_at.unwrap().ends_with('Z'));
    }

    #[test]
    fn claude_context_falls_back_to_the_percentage() {
        let payload = json!({
            "session_id": "abc",
            "context_window": {
                "context_window_size": 200_000,
                "used_percentage": 8,
                "current_usage": null
            }
        });
        let observation = parse(HarnessKind::Claude, None, &payload).unwrap();

        assert_eq!(observation.context_used_tokens, Some(16_000));
    }

    #[test]
    fn claude_context_falls_back_to_the_percentage_when_current_usage_is_absent() {
        let payload = json!({
            "session_id": "abc",
            "context_window": { "context_window_size": 200_000, "used_percentage": 10 }
        });
        let observation = parse(HarnessKind::Claude, None, &payload).unwrap();

        assert_eq!(observation.context_used_tokens, Some(20_000));
        assert_eq!(observation.context_limit_tokens, Some(200_000));
    }

    #[test]
    fn claude_context_is_unknown_with_neither_usage_nor_a_percentage() {
        let payload = json!({
            "session_id": "abc",
            "context_window": { "context_window_size": 200_000 }
        });
        let observation = parse(HarnessKind::Claude, None, &payload).unwrap();

        assert_eq!(observation.context_used_tokens, None);
        assert_eq!(observation.context_limit_tokens, Some(200_000));
    }

    #[test]
    fn claude_drops_a_window_that_already_reset() {
        let payload = json!({
            "session_id": "abc",
            "rate_limits": {
                "five_hour": { "used_percentage": 90.0, "resets_at": future(-60) },
                "seven_day": { "used_percentage": 10.0, "resets_at": future(60) }
            }
        });
        let plan = parse(HarnessKind::Claude, None, &payload)
            .unwrap()
            .plan
            .unwrap();

        assert_eq!(plan.five_hour_percent, None);
        assert_eq!(plan.five_hour_resets_at, None);
        assert_eq!(plan.week_percent, Some(10.0));
    }

    #[test]
    fn claude_without_optional_sections_still_identifies_the_session() {
        let observation =
            parse(HarnessKind::Claude, None, &json!({ "session_id": "abc" })).unwrap();

        assert_eq!(observation.native_session.as_deref(), Some("abc"));
        assert_eq!(observation.cost_usd, None);
        assert_eq!(observation.context_used_tokens, None);
        let plan = observation.plan.unwrap();
        assert_eq!(plan.five_hour_percent, None);
    }

    #[test]
    fn codex_hooks_identify_the_session_without_usage() {
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "c1",
            "cwd": "/work",
            "model": "gpt-6-astra",
            "turn_id": "t1"
        });
        let observation = parse(HarnessKind::Codex, None, &payload).unwrap();

        assert_eq!(observation.event.as_deref(), Some("Stop"));
        assert_eq!(observation.native_session.as_deref(), Some("c1"));
        assert_eq!(observation.model.as_deref(), Some("gpt-6-astra"));
        assert_eq!(observation.cost_usd, None);
    }

    #[test]
    fn hermes_takes_the_model_from_extra() {
        let payload = json!({
            "hook_event_name": "on_session_start",
            "session_id": "h1",
            "cwd": "/work",
            "extra": { "model": "deepseek/v4", "platform": "cli" }
        });
        let observation = parse(HarnessKind::Hermes, None, &payload).unwrap();

        assert_eq!(observation.event.as_deref(), Some("on_session_start"));
        assert_eq!(observation.model.as_deref(), Some("deepseek/v4"));
    }

    #[test]
    fn opencode_session_events_carry_the_running_cost() {
        let payload = json!({
            "type": "session.updated",
            "properties": {
                "sessionID": "ses_1",
                "info": {
                    "id": "ses_1",
                    "directory": "/work",
                    "cost": 2.1016,
                    "model": { "id": "glm-5.3", "providerID": "opencode-go" }
                }
            }
        });
        let observation = parse(HarnessKind::OpenCode, None, &payload).unwrap();

        assert_eq!(observation.event.as_deref(), Some("session.updated"));
        assert_eq!(observation.native_session.as_deref(), Some("ses_1"));
        assert_eq!(observation.cwd.as_deref(), Some("/work"));
        assert_eq!(observation.model.as_deref(), Some("glm-5.3"));
        assert_eq!(observation.cost_usd, Some(2.1016));
    }

    #[test]
    fn opencode_idle_events_only_identify_the_session() {
        let payload = json!({
            "type": "session.idle",
            "properties": { "sessionID": "ses_1" }
        });
        let observation = parse(HarnessKind::OpenCode, None, &payload).unwrap();

        assert_eq!(observation.native_session.as_deref(), Some("ses_1"));
        assert_eq!(observation.cost_usd, None);
    }

    #[test]
    fn the_event_argument_wins_over_the_payload() {
        let payload = json!({ "hook_event_name": "Stop", "session_id": "c1" });

        assert_eq!(
            parse(HarnessKind::Codex, Some("SessionEnd"), &payload)
                .unwrap()
                .event
                .as_deref(),
            Some("SessionEnd")
        );
    }

    #[test]
    fn non_objects_are_ignored() {
        assert!(parse(HarnessKind::Claude, None, &json!("text")).is_none());
        assert!(parse(HarnessKind::Claude, None, &json!([1, 2])).is_none());
    }
}
