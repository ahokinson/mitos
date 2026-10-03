mod claude;
mod codex;
mod hermes;
mod opencode;

use super::locations::Locations;
use super::reports::{HookStatus, InitOutcome, InitResult};

pub const HARNESSES: [&str; 4] = ["claude", "codex", "hermes", "opencode"];

pub fn status(locations: &Locations) -> Vec<HookStatus> {
    vec![
        claude::status(&locations.claude_settings),
        codex::status(&locations.codex_hooks, &locations.codex_config),
        hermes::status(&locations.hermes_config, &locations.hermes_allowlist),
        opencode::status(&locations.opencode_plugin),
    ]
}

/// `only` empty means every harness; an unknown name is reported, not ignored.
pub fn init(locations: &Locations, only: &[String]) -> Vec<InitOutcome> {
    let wanted = |harness: &str| only.is_empty() || only.iter().any(|name| name == harness);
    let mut outcomes = Vec::new();
    if wanted("claude") {
        outcomes.push(claude::init(&locations.claude_settings));
    }
    if wanted("codex") {
        outcomes.push(codex::init(&locations.codex_hooks));
    }
    if wanted("hermes") {
        outcomes.push(hermes::init(&locations.hermes_config));
    }
    if wanted("opencode") {
        outcomes.push(opencode::init(&locations.opencode_plugin));
    }
    for name in only
        .iter()
        .filter(|name| !HARNESSES.contains(&name.as_str()))
    {
        outcomes.push(InitOutcome::new(
            name,
            InitResult::Skipped,
            format!("unknown harness; choose from {}", HARNESSES.join(", ")),
        ));
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{init, status};
    use crate::hooks::files::scratch_dir;
    use crate::hooks::locations::Locations;
    use crate::hooks::reports::InitResult;

    fn locations(dir: &std::path::Path) -> Locations {
        Locations::from_dirs(
            &dir.join("claude"),
            &dir.join("codex"),
            &dir.join("hermes"),
            &dir.join("opencode"),
        )
    }

    #[test]
    fn status_lists_every_harness_in_a_fixed_order() {
        let dir = scratch_dir();

        let names: Vec<_> = status(&locations(&dir))
            .into_iter()
            .map(|s| s.harness)
            .collect();

        assert_eq!(names, ["claude", "codex", "hermes", "opencode"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_can_be_limited_to_named_harnesses_and_flags_unknown_ones() {
        let dir = scratch_dir();

        let outcomes = init(&locations(&dir), &["codex".into(), "bogus".into()]);

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].harness, "codex");
        assert_eq!(outcomes[0].result, InitResult::Installed);
        assert_eq!(outcomes[1].harness, "bogus");
        assert_eq!(outcomes[1].result, InitResult::Skipped);
        assert!(!dir.join("claude").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn opencode_prefers_the_plugin_directory_the_user_already_has() {
        let dir = scratch_dir();
        fs::create_dir_all(dir.join("opencode").join("plugin")).unwrap();

        let plugin = locations(&dir).opencode_plugin;

        assert_eq!(plugin, dir.join("opencode").join("plugin").join("mitos.js"));
        fs::remove_dir_all(dir).unwrap();
    }
}
