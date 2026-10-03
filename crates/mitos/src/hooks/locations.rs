use std::path::{Path, PathBuf};

/// Where each harness keeps the config Mitos installs its hook into.
#[derive(Clone, Debug)]
pub struct Locations {
    pub claude_settings: PathBuf,
    pub codex_hooks: PathBuf,
    pub codex_config: PathBuf,
    pub hermes_config: PathBuf,
    pub hermes_allowlist: PathBuf,
    pub opencode_plugin: PathBuf,
}

impl Locations {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let directory = |variable: &str, fallback: PathBuf| {
            std::env::var_os(variable).map_or(fallback, PathBuf::from)
        };
        let config_home = directory("XDG_CONFIG_HOME", home.join(".config"));
        Self::from_dirs(
            &directory("CLAUDE_CONFIG_DIR", home.join(".claude")),
            &directory("CODEX_HOME", home.join(".codex")),
            &directory("HERMES_HOME", home.join(".hermes")),
            &config_home.join("opencode"),
        )
    }

    pub fn from_dirs(claude: &Path, codex: &Path, hermes: &Path, opencode: &Path) -> Self {
        // OpenCode loads either spelling; keep to whichever the user already has.
        let plugin_directory = ["plugin", "plugins"]
            .iter()
            .map(|name| opencode.join(name))
            .find(|path| path.is_dir())
            .unwrap_or_else(|| opencode.join("plugins"));
        Self {
            claude_settings: claude.join("settings.json"),
            codex_hooks: codex.join("hooks.json"),
            codex_config: codex.join("config.toml"),
            hermes_config: hermes.join("config.yaml"),
            hermes_allowlist: hermes.join("shell-hooks-allowlist.json"),
            opencode_plugin: plugin_directory.join("mitos.js"),
        }
    }
}
