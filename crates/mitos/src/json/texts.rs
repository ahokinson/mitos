use serde_json::Value;

use super::records::{first_present, parsed_container};

const CLIP_LIMIT: usize = 160;

pub fn text_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => match parsed_container(value) {
            Some(parsed) => text_value(&parsed),
            None => Some(text.trim().to_string()).filter(|trimmed| !trimmed.is_empty()),
        },
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(text_value)
                .collect::<Vec<_>>()
                .join("\n");
            Some(text).filter(|joined| !joined.is_empty())
        }
        Value::Object(record) => {
            let direct = first_present(&[
                record.get("text"),
                record.get("content"),
                record.get("output_text"),
                record.get("input_text"),
            ]);
            direct
                .and_then(text_value)
                .or_else(|| record.get("parts").and_then(text_value))
        }
        _ => None,
    }
}

pub fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= CLIP_LIMIT {
        return flat;
    }
    let mut clipped: String = flat.chars().take(CLIP_LIMIT - 1).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn text_value_flattens_blocks_and_json_strings() {
        assert_eq!(text_value(&json!("  hi ")).as_deref(), Some("hi"));
        assert_eq!(
            text_value(&json!([{ "text": "a" }, { "text": "b" }])).as_deref(),
            Some("a\nb")
        );
        assert_eq!(
            text_value(&json!("{\"text\":\"inner\"}")).as_deref(),
            Some("inner")
        );
        assert_eq!(
            text_value(&json!({ "parts": [{ "text": "p" }] })).as_deref(),
            Some("p")
        );
        assert_eq!(text_value(&json!("   ")), None);
        assert_eq!(text_value(&json!(3)), None);
    }

    #[test]
    fn clip_collapses_whitespace_and_truncates() {
        assert_eq!(clip("a \n  b"), "a b");
        let long = "x".repeat(200);
        let clipped = clip(&long);
        assert_eq!(clipped.chars().count(), CLIP_LIMIT);
        assert!(clipped.ends_with('…'));
    }
}
