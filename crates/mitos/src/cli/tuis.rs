use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

fn bun_program() -> OsString {
    std::env::var_os("MITOS_BUN").unwrap_or_else(|| "bun".into())
}

pub fn tui_command() -> Result<Command> {
    let bun = bun_program();
    if let Some(tui) = std::env::var_os("MITOS_TUI").map(PathBuf::from) {
        return Ok(override_command(tui, bun));
    }
    Ok(packaged_command(Path::new("tui/main.mjs"), bun)?
        .unwrap_or_else(|| Command::new("mitos-tui")))
}

fn override_command(tui: PathBuf, bun: OsString) -> Command {
    if tui
        .extension()
        .is_some_and(|extension| extension == "ts" || extension == "tsx")
    {
        let mut command = Command::new(bun);
        if let Some(directory) = tui.parent() {
            command.current_dir(directory);
        }
        command.arg("--preload").arg("@opentui/solid/preload");
        command.arg(tui);
        return command;
    }
    Command::new(tui)
}

fn packaged_command(entry: &Path, bun: OsString) -> Result<Option<Command>> {
    let Some(program) = resolve(entry)? else {
        return Ok(None);
    };
    Ok(Some(bun_command(bun, program)))
}

fn bun_command(bun: OsString, program: PathBuf) -> Command {
    let mut command = Command::new(bun);
    command.arg(program);
    command
}

fn resolve(entry: &Path) -> Result<Option<PathBuf>> {
    if let Some(root) = std::env::var_os("MITOS_LIBRARY_DIR") {
        return Ok(resolve_with(
            entry,
            Some(Path::new(&root)),
            Path::new(""),
            &Path::exists,
        ));
    }
    let executable = std::env::current_exe()?;
    Ok(resolve_with(entry, None, &executable, &Path::exists))
}

fn resolve_with(
    entry: &Path,
    library_dir: Option<&Path>,
    executable: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(root) = library_dir {
        let entry = root.join(entry);
        return exists(&entry).then_some(entry);
    }
    let prefix = executable.parent()?.parent()?;
    let entry = prefix.join("lib").join("mitos").join(entry);
    exists(&entry).then_some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_entries_run_through_bun() {
        let command = bun_command(
            OsString::from("bun"),
            PathBuf::from("/tmp/mitos/tui/main.mjs"),
        );
        assert_eq!(command.get_program(), "bun");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["/tmp/mitos/tui/main.mjs"]
        );
    }

    #[test]
    fn typescript_overrides_run_with_bun_and_the_opentui_preload() {
        let command = override_command(
            PathBuf::from("/work/tui/src/entry.tsx"),
            OsString::from("custom-bun"),
        );

        assert_eq!(command.get_program(), "custom-bun");
        assert_eq!(command.get_current_dir(), Some(Path::new("/work/tui/src")));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--preload",
                "@opentui/solid/preload",
                "/work/tui/src/entry.tsx"
            ]
        );
    }

    #[test]
    fn executable_overrides_run_directly() {
        let command = override_command(
            PathBuf::from("/opt/mitos-tui"),
            OsString::from("unused-bun"),
        );

        assert_eq!(command.get_program(), "/opt/mitos-tui");
        assert!(command.get_args().next().is_none());
    }

    #[test]
    fn library_override_resolves_a_packaged_entry() {
        let entry = resolve_with(
            Path::new("tui/main.mjs"),
            Some(Path::new("/opt/mitos/lib")),
            Path::new("/unused"),
            &|path| path == Path::new("/opt/mitos/lib/tui/main.mjs"),
        );

        assert_eq!(entry, Some(PathBuf::from("/opt/mitos/lib/tui/main.mjs")));
    }

    #[test]
    fn packaged_lookup_uses_the_executable_prefix() {
        let entry = resolve_with(
            Path::new("tui/main.mjs"),
            None,
            Path::new("/opt/mitos/bin/mitos"),
            &|path| path == Path::new("/opt/mitos/lib/mitos/tui/main.mjs"),
        );

        assert_eq!(
            entry,
            Some(PathBuf::from("/opt/mitos/lib/mitos/tui/main.mjs"))
        );
    }

    #[test]
    fn missing_packaged_entries_fall_back_to_the_tui_program() {
        let command = packaged_command(Path::new("tui/not-present.mjs"), OsString::from("bun"));

        assert!(command.unwrap().is_none());
    }
}
