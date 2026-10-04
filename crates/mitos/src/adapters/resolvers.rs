use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

fn bun_program() -> OsString {
    std::env::var_os("MITOS_BUN").unwrap_or_else(|| "bun".into())
}

pub fn tui_command() -> Result<Command> {
    if let Some(tui) = std::env::var_os("MITOS_TUI").map(PathBuf::from) {
        if tui
            .extension()
            .is_some_and(|extension| extension == "ts" || extension == "tsx")
        {
            let mut command = Command::new(bun_program());
            if let Some(directory) = tui.parent() {
                command.current_dir(directory);
            }
            command.arg("--preload").arg("@opentui/solid/preload");
            command.arg(&tui);
            return Ok(command);
        }
        return Ok(Command::new(tui));
    }
    Ok(packaged_command(Path::new("tui/main.mjs"))?.unwrap_or_else(|| Command::new("mitos-tui")))
}

pub fn adapter_command(harness: &str, adapter_dir: Option<&Path>) -> Command {
    let (executable, args) = external_adapter_command(harness, adapter_dir);
    let mut command = Command::new(executable);
    command.args(args);
    command
}

fn external_adapter_command(harness: &str, adapter_dir: Option<&Path>) -> (PathBuf, Vec<OsString>) {
    let name = format!("mitos-{harness}");
    let Some(directory) = adapter_dir else {
        return (PathBuf::from(name), Vec::new());
    };
    let executable = directory.join(&name);
    if executable.exists() {
        return (executable, Vec::new());
    }
    let source = directory.join(format!("{harness}.ts"));
    if source.exists() {
        return (PathBuf::from("bun"), vec![source.into_os_string()]);
    }
    (executable, Vec::new())
}

fn packaged_command(entry: &Path) -> Result<Option<Command>> {
    let Some(program) = resolve(entry)? else {
        return Ok(None);
    };
    Ok(Some(bun_command(program)))
}

fn bun_command(program: PathBuf) -> Command {
    let mut command = Command::new(bun_program());
    command.arg(program);
    command
}

fn resolve(entry: &Path) -> Result<Option<PathBuf>> {
    if let Some(root) = std::env::var_os("MITOS_LIBRARY_DIR") {
        let entry = PathBuf::from(root).join(entry);
        return Ok(entry.exists().then_some(entry));
    }
    let executable = std::env::current_exe()?;
    let Some(bin_dir) = executable.parent() else {
        return Ok(None);
    };
    let Some(prefix) = bin_dir.parent() else {
        return Ok(None);
    };
    let entry = prefix.join("lib").join("mitos").join(entry);
    Ok(entry.exists().then_some(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_entries_run_through_bun() {
        let command = bun_command(PathBuf::from("/tmp/mitos/adapters/codex.mjs"));
        assert_eq!(command.get_program(), "bun");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["/tmp/mitos/adapters/codex.mjs"]
        );
    }

    #[test]
    fn adapter_source_is_run_with_bun_when_no_binary_exists() {
        let directory =
            std::env::temp_dir().join(format!("mitos-adapter-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("codex.ts");
        std::fs::write(&source, "// adapter").unwrap();

        let (program, args) = external_adapter_command("codex", Some(&directory));
        assert_eq!(program, PathBuf::from("bun"));
        assert_eq!(args, vec![source.into_os_string()]);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
