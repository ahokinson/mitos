use std::fs;
use std::path::Path;

const DEFAULT_HANDOFF_MAX_INLINE_BYTES: usize = 48 * 1024;

/// Missing or invalid config falls back to the transport-safe default.
pub fn handoff_max_inline_bytes(config_root: &Path) -> usize {
    let Ok(source) = fs::read_to_string(config_root.join("mitos.toml")) else {
        return DEFAULT_HANDOFF_MAX_INLINE_BYTES;
    };
    let Ok(value) = source.parse::<toml::Value>() else {
        return DEFAULT_HANDOFF_MAX_INLINE_BYTES;
    };
    value
        .get("handoff")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("max_inline_bytes"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_HANDOFF_MAX_INLINE_BYTES)
}
