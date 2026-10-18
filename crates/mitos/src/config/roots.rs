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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_root_uses_home_when_xdg_config_home_is_absent() {
        let home = std::env::temp_dir().join(format!("mitos-root-test-{}", crate::domain::id()));
        let previous_home = std::env::var_os("HOME");
        let previous_xdg = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        assert_eq!(resolve_root(None), home.join(".config/mitos"));

        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_xdg {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }
}
