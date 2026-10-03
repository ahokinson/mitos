use std::path::PathBuf;

pub fn resolve_root(override_dir: Option<PathBuf>) -> PathBuf {
    match override_dir {
        Some(path) => path,
        None => std::env::var_os("XDG_CONFIG_HOME")
            .map_or_else(
                || PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"),
                PathBuf::from,
            )
            .join("mitos"),
    }
}
