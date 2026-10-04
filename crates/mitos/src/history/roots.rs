use std::path::PathBuf;

/// A harness's config home: `env_var` when set, else `default_dir` under `$HOME`.
pub fn harness_home(env_var: &str, default_dir: &str) -> PathBuf {
    std::env::var_os(env_var).map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(default_dir),
        PathBuf::from,
    )
}
