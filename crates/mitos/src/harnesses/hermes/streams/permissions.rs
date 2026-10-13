use serde_json::{Value, json};

use crate::domain::RequestKind;
use crate::json::{clip, string_value};
use crate::rpc::RpcId;
use crate::wire::events::{AdapterEvent, kinds};

pub const REQUEST_PERMISSION: &str = "session/request_permission";

/// A permission request awaiting a host decision.
#[derive(Clone, Debug)]
pub struct PendingPermission {
    pub rpc_id: RpcId,
    options: Vec<PermissionOption>,
}

#[derive(Clone, Debug)]
struct PermissionOption {
    option_id: String,
    kind: String,
}

/// `None` when the request has nothing the host could decide.
pub fn permission_request(
    rpc_id: &RpcId,
    params: &Value,
) -> Option<(AdapterEvent, PendingPermission)> {
    let options: Vec<PermissionOption> = params
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|option| {
            Some(PermissionOption {
                option_id: string_value(option.get("optionId"))?.into(),
                kind: string_value(option.get("kind")).unwrap_or("").into(),
            })
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    let title = clip(
        string_value(params.get("toolCall").and_then(|call| call.get("title")))
            .unwrap_or("Permission requested"),
    );
    let payload = json!({
        "id": rpc_id.to_text(),
        "kind": RequestKind::Permission.as_str(),
        "title": title,
        "method": REQUEST_PERMISSION,
        "params": if params.is_object() { params.clone() } else { json!({}) },
    });
    let event = AdapterEvent {
        payload: Some(payload),
        ..AdapterEvent::new(kinds::REQUEST)
    };
    let pending = PendingPermission {
        rpc_id: rpc_id.clone(),
        options,
    };
    Some((event, pending))
}

/// The `session/request_permission` result for the TUI's `{allow}` answer.
pub fn outcome_for(pending: &PendingPermission, response: &Value) -> Value {
    let allow = response.get("allow") == Some(&Value::Bool(true));
    let preferred = if allow {
        ["allow_once", "allow_always"]
    } else {
        ["reject_once", "reject_always"]
    };
    preferred
        .iter()
        .find_map(|kind| pending.options.iter().find(|option| option.kind == *kind))
        .map_or_else(
            || json!({ "outcome": { "outcome": "cancelled" } }),
            |option| json!({ "outcome": { "outcome": "selected", "optionId": option.option_id } }),
        )
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use super::*;

    fn options() -> Value {
        json!([
            { "optionId": "ao", "name": "Allow once", "kind": "allow_once" },
            { "optionId": "aa", "name": "Always", "kind": "allow_always" },
            { "optionId": "ro", "name": "Reject", "kind": "reject_once" }
        ])
    }

    #[test]
    fn a_permission_request_becomes_a_request_and_answers_with_an_option_id() {
        let (event, pending) = permission_request(
            &RpcId::Number(7),
            &json!({ "toolCall": { "toolCallId": "t1", "title": "Run rm -rf build" }, "options": options() }),
        )
        .unwrap();
        assert_eq!(event.event, kinds::REQUEST);
        let payload = event.payload.unwrap();
        assert_eq!(payload["id"], "7");
        assert_eq!(payload["kind"], "permission");
        assert_eq!(payload["title"], "Run rm -rf build");
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": true })),
            json!({ "outcome": { "outcome": "selected", "optionId": "ao" } })
        );
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": false })),
            json!({ "outcome": { "outcome": "selected", "optionId": "ro" } })
        );
    }

    #[test]
    fn allow_falls_back_to_allow_always_and_a_missing_reject_cancels() {
        let (_, pending) = permission_request(
            &RpcId::Number(1),
            &json!({ "options": [{ "optionId": "aa", "kind": "allow_always" }] }),
        )
        .unwrap();
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": true })),
            json!({ "outcome": { "outcome": "selected", "optionId": "aa" } })
        );
        assert_eq!(
            outcome_for(&pending, &json!({ "allow": false })),
            json!({ "outcome": { "outcome": "cancelled" } })
        );
    }

    #[test]
    fn a_permission_request_with_no_options_cannot_be_answered() {
        assert!(permission_request(&RpcId::Number(1), &json!({ "options": [] })).is_none());
    }
}
