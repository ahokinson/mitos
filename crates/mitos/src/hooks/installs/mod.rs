mod claude;
mod codex;
mod hermes;
mod opencode;

use super::locations::Locations;
use super::reports::{HookStatus, InitOutcome, InitResult};
use crate::domain::HarnessKind;

fn harness_names() -> Vec<&'static str> {
    HarnessKind::ALL
        .into_iter()
        .map(HarnessKind::as_str)
        .collect()
}

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
    let mut outcomes: Vec<InitOutcome> = HarnessKind::ALL
        .into_iter()
        .filter(|harness| only.is_empty() || only.iter().any(|name| name == harness.as_str()))
        .map(|harness| match harness {
            HarnessKind::Claude => claude::init(&locations.claude_settings),
            HarnessKind::Codex => codex::init(&locations.codex_hooks),
            HarnessKind::Hermes => hermes::init(&locations.hermes_config),
            HarnessKind::OpenCode => opencode::init(&locations.opencode_plugin),
        })
        .collect();
    for name in only
        .iter()
        .filter(|name| name.parse::<HarnessKind>().is_err())
    {
        outcomes.push(InitOutcome::new(
            name,
            InitResult::Skipped,
            format!(
                "unknown harness; choose from {}",
                harness_names().join(", ")
            ),
        ));
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{HarnessKind, init, status};
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

        assert_eq!(names, HarnessKind::ALL);
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
