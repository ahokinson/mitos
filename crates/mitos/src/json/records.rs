use serde_json::{Map, Value};

pub type Record = Map<String, Value>;

pub fn field<'a>(record: Option<&'a Record>, key: &str) -> Option<&'a Value> {
    record?.get(key)
}

pub fn object_field<'a>(record: Option<&'a Record>, key: &str) -> Option<&'a Record> {
    field(record, key)?.as_object()
}

/// The first value that is present and not `null`, like a `??` chain.
pub fn first_present<'a>(values: &[Option<&'a Value>]) -> Option<&'a Value> {
    values
        .iter()
        .flatten()
        .find(|value| !value.is_null())
        .copied()
}

pub fn string_value(value: Option<&Value>) -> Option<&str> {
    value?.as_str().filter(|text| !text.is_empty())
}

pub fn number_value(value: Option<&Value>) -> Option<f64> {
    value?.as_f64().filter(|number| number.is_finite())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn count_value(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|number| number.is_finite() && *number >= 0.0)
            .map(|number| number as u64)
    })
}

/// A JSON object or array, either as given or decoded from a string.
pub fn parsed_container(value: &Value) -> Option<Value> {
    match value {
        Value::Object(_) | Value::Array(_) => Some(value.clone()),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .filter(|parsed| parsed.is_object() || parsed.is_array()),
        _ => None,
    }
}

/// Depth-first, direct keys before nested values.
pub fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    let children: Vec<&Value> = match value {
        Value::Object(record) => {
            for key in keys {
                if let Some(direct) = string_value(record.get(*key)) {
                    return Some(direct.to_string());
                }
            }
            record.values().collect()
        }
        Value::Array(items) => items.iter().collect(),
        _ => return None,
    };
    children
        .into_iter()
        .find_map(|nested| find_string(nested, keys))
}

pub fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn find_string_prefers_direct_keys_then_descends() {
        let value = json!({ "a": { "cwd": "/nested" }, "workdir": "/direct" });
        assert_eq!(
            find_string(&value, &["cwd", "workdir"]).as_deref(),
            Some("/direct")
        );
        let nested = json!({ "list": [{ "cwd": "/in-array" }] });
        assert_eq!(find_string(&nested, &["cwd"]).as_deref(), Some("/in-array"));
        assert_eq!(find_string(&json!({ "cwd": "" }), &["cwd"]), None);
    }

    #[test]
    fn first_present_skips_null_and_missing() {
        let null = Value::Null;
        let text = json!("x");
        assert_eq!(
            first_present(&[None, Some(&null), Some(&text)]),
            Some(&text)
        );
    }

    #[test]
    fn count_value_accepts_whole_and_fractional_non_negative_numbers() {
        assert_eq!(count_value(Some(&json!(7))), Some(7));
        assert_eq!(count_value(Some(&json!(7.9))), Some(7));
        assert_eq!(count_value(Some(&json!(-1))), None);
        assert_eq!(count_value(Some(&json!("7"))), None);
    }
}
