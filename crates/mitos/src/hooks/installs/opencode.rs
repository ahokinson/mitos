use std::path::Path;

use crate::hooks::files::{blocked_reason, read_text, write_atomic};
use crate::hooks::reports::{HookStatus, InitOutcome, InitResult, Trust};

const HARNESS: &str = "opencode";
const MARKER: &str = "mitos hook opencode";

/// The harness has no command hooks, so Mitos installs a plugin that forwards
/// session events. `session.updated` fires on every streamed update, so it is
/// throttled; the idle event flushes the latest one so the final cost lands.
const PLUGIN: &str = r#"// Installed by `mitos hooks init`. Forwards OpenCode session events to Mitos.
const INTERVAL_MS = 5000;
const latest = new Map();
const lastSent = new Map();

const send = ($, type, properties) => {
  const payload = JSON.stringify({ type, properties });
  return $`mitos hook opencode --event ${type} < ${new Response(payload)}`
    .quiet()
    .nothrow();
};

export const MitosHooks = async ({ $ }) => ({
  event: async ({ event }) => {
    const { type, properties } = event;
    if (type === "session.created") return send($, type, properties);
    if (type === "session.updated") {
      const id = properties?.info?.id ?? properties?.sessionID;
      latest.set(id, properties);
      if (Date.now() - (lastSent.get(id) ?? 0) < INTERVAL_MS) return;
      lastSent.set(id, Date.now());
      return send($, type, properties);
    }
    if (type === "session.idle") {
      const id = properties?.sessionID;
      const last = latest.get(id);
      latest.delete(id);
      lastSent.delete(id);
      if (last) await send($, "session.updated", last);
      return send($, type, properties);
    }
  },
});
"#;

pub fn status(plugin: &Path) -> HookStatus {
    HookStatus {
        harness: HARNESS.into(),
        target: plugin.display().to_string(),
        installed: read_text(plugin).is_some_and(|text| text.contains(MARKER)),
        trust: Trust::NotApplicable,
        blocked_by: blocked_reason(plugin),
        problems: Vec::new(),
        last_seen: None,
    }
}

pub fn init(plugin: &Path) -> InitOutcome {
    if let Some(reason) = blocked_reason(plugin) {
        return InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {reason}"),
        )
        .with_snippet(PLUGIN);
    }
    match read_text(plugin) {
        Some(text) if text.contains(MARKER) => {
            return InitOutcome::new(
                HARNESS,
                InitResult::AlreadyInstalled,
                "the plugin is already installed",
            );
        }
        Some(_) => {
            return InitOutcome::new(
                HARNESS,
                InitResult::Skipped,
                format!(
                    "not changed: {} exists and is not Mitos's plugin",
                    plugin.display()
                ),
            );
        }
        None => {}
    }
    match write_atomic(plugin, PLUGIN) {
        Ok(()) => InitOutcome::new(
            HARNESS,
            InitResult::Installed,
            "installed the plugin; restart OpenCode to load it",
        ),
        Err(error) => InitOutcome::new(
            HARNESS,
            InitResult::Skipped,
            format!("not changed: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::hooks::files::scratch_dir;
    use crate::hooks::reports::InitResult;

    use super::{PLUGIN, init, status};

    #[test]
    fn init_writes_the_plugin_and_status_sees_it() {
        let dir = scratch_dir();
        let plugin = dir.join("plugins").join("mitos.js");
        assert!(!status(&plugin).installed);

        let outcome = init(&plugin);

        assert_eq!(outcome.result, InitResult::Installed);
        assert_eq!(fs::read_to_string(&plugin).unwrap(), PLUGIN);
        assert!(status(&plugin).installed);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_twice_changes_nothing_the_second_time() {
        let dir = scratch_dir();
        let plugin = dir.join("mitos.js");
        init(&plugin);

        assert_eq!(init(&plugin).result, InitResult::AlreadyInstalled);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_foreign_file_with_the_same_name_is_left_alone() {
        let dir = scratch_dir();
        let plugin = dir.join("mitos.js");
        fs::write(&plugin, "export const Mine = () => ({})").unwrap();

        assert_eq!(init(&plugin).result, InitResult::Skipped);
        assert_eq!(
            fs::read_to_string(&plugin).unwrap(),
            "export const Mine = () => ({})"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_read_only_directory_is_reported_with_the_plugin_source() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir();
        let locked = dir.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let plugin = locked.join("mitos.js");

        assert!(status(&plugin).blocked_by.is_some());
        let outcome = init(&plugin);

        assert_eq!(outcome.result, InitResult::Skipped);
        assert!(outcome.snippet.unwrap().contains("mitos hook opencode"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
