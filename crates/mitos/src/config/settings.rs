use std::fs;
use std::path::Path;

const DEFAULT_HANDOFF_MAX_INLINE_BYTES: usize = 256 * 1024;

/// Linux caps one argv entry at 128 KiB (`MAX_ARG_STRLEN`); launch handoffs
/// travel as one, so they stay safely under it.
const MAX_LAUNCH_ARGUMENT_BYTES: usize = 120 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandoffLimits {
    /// Handoffs sent over a protocol: headless turns and summaries.
    pub inline_bytes: usize,
    /// Handoffs passed on a harness command line by `mitos enter`.
    pub launch_bytes: usize,
}

/// Missing or invalid config falls back to the default.
pub fn handoff_limits(config_root: &Path) -> HandoffLimits {
    let inline_bytes = configured_inline_bytes(config_root);
    HandoffLimits {
        inline_bytes,
        launch_bytes: inline_bytes.min(MAX_LAUNCH_ARGUMENT_BYTES),
    }
}

fn configured_inline_bytes(config_root: &Path) -> usize {
    let Ok(source) = fs::read_to_string(config_root.join("mitos.toml")) else {
        return DEFAULT_HANDOFF_MAX_INLINE_BYTES;
    };
    let Ok(document) = source.parse::<toml::Table>() else {
        return DEFAULT_HANDOFF_MAX_INLINE_BYTES;
    };
    document
        .get("handoff")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("max_inline_bytes"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_HANDOFF_MAX_INLINE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::id;

    fn limits_for(config: Option<&str>) -> HandoffLimits {
        let root = std::env::temp_dir().join(format!("mitos-settings-test-{}", id()));
        fs::create_dir_all(&root).unwrap();
        if let Some(config) = config {
            fs::write(root.join("mitos.toml"), config).unwrap();
        }
        let limits = handoff_limits(&root);
        fs::remove_dir_all(&root).unwrap();
        limits
    }

    #[test]
    fn defaults_apply_without_config_and_launch_stays_under_the_argument_limit() {
        let limits = limits_for(None);
        assert_eq!(limits.inline_bytes, 256 * 1024);
        assert_eq!(limits.launch_bytes, 120 * 1024);
        assert!(limits.launch_bytes < 128 * 1024);
    }

    #[test]
    fn a_configured_cap_applies_to_both_paths_until_the_argument_limit() {
        let small = limits_for(Some("[handoff]\nmax_inline_bytes = 4096\n"));
        assert_eq!((small.inline_bytes, small.launch_bytes), (4096, 4096));
        let large = limits_for(Some("[handoff]\nmax_inline_bytes = 1048576\n"));
        assert_eq!(large.inline_bytes, 1_048_576);
        assert_eq!(large.launch_bytes, 120 * 1024);
    }

    #[test]
    fn invalid_config_falls_back_to_the_defaults() {
        for config in [
            "not = [toml",
            "[handoff]\nmax_inline_bytes = 0\n",
            "[handoff]\nmax_inline_bytes = -5\n",
            "[handoff]\nmax_inline_bytes = \"big\"\n",
        ] {
            assert_eq!(
                limits_for(Some(config)).inline_bytes,
                256 * 1024,
                "{config}"
            );
        }
    }
}
