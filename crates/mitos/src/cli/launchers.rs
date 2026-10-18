use std::process::Command;

use anyhow::{Result, anyhow, bail};

use super::tuis::tui_command;
use crate::git::Checkout;

pub fn launch_tui(state_dir: Option<&std::path::Path>) -> Result<()> {
    let workspace = Checkout::open(std::env::current_dir()?)?;
    let mut command = tui_command()?;
    configure_tui(
        &mut command,
        &std::env::current_exe()?,
        workspace.root(),
        &workspace.key(),
        state_dir,
    );
    run_tui(&mut command, |command| {
        command.status().map(|status| {
            if status.success() {
                TuiExit::Success
            } else {
                TuiExit::Failure(status.to_string())
            }
        })
    })
}

fn configure_tui(
    command: &mut Command,
    core: &std::path::Path,
    workspace_root: &std::path::Path,
    workspace_key: &str,
    state_dir: Option<&std::path::Path>,
) {
    command.env("MITOS_CORE", core);
    command.env("MITOS_WORKSPACE_ROOT", workspace_root);
    command.env("MITOS_WORKSPACE_KEY", workspace_key);
    if let Some(state_dir) = state_dir {
        command.env("MITOS_STATE_DIR", state_dir);
    }
}

enum TuiExit {
    Success,
    Failure(String),
}

fn run_tui(
    command: &mut Command,
    execute: impl FnOnce(&mut Command) -> std::io::Result<TuiExit>,
) -> Result<()> {
    match execute(command).map_err(|error| {
        anyhow!(
            "could not start Mitos TUI: {error}. Build with `bun run build`, or set MITOS_TUI to packages/tui/src/app/entries.tsx during development."
        )
    })? {
        TuiExit::Success => Ok(()),
        TuiExit::Failure(status) => bail!("Mitos TUI exited with {status}"),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::OsStr;
    use std::io;
    use std::path::Path;

    use super::*;

    fn environment<'a>(command: &'a Command, name: &str) -> Option<&'a OsStr> {
        command
            .get_envs()
            .find_map(|(key, value)| (key == name).then_some(value).flatten())
    }

    #[test]
    fn tui_configuration_sets_the_runtime_context() {
        let mut command = Command::new("mitos-tui");
        configure_tui(
            &mut command,
            Path::new("/opt/mitos/bin/mitos"),
            Path::new("/work/project"),
            "workspace-key",
            Some(Path::new("/var/lib/mitos")),
        );

        assert_eq!(
            environment(&command, "MITOS_CORE"),
            Some(OsStr::new("/opt/mitos/bin/mitos"))
        );
        assert_eq!(
            environment(&command, "MITOS_WORKSPACE_ROOT"),
            Some(OsStr::new("/work/project"))
        );
        assert_eq!(
            environment(&command, "MITOS_WORKSPACE_KEY"),
            Some(OsStr::new("workspace-key"))
        );
        assert_eq!(
            environment(&command, "MITOS_STATE_DIR"),
            Some(OsStr::new("/var/lib/mitos"))
        );
    }

    #[test]
    fn tui_start_errors_include_the_development_guidance() {
        let mut command = Command::new("mitos-tui");
        let error =
            run_tui(&mut command, |_| Err(io::Error::other("permission denied"))).unwrap_err();

        assert_eq!(
            error.to_string(),
            "could not start Mitos TUI: permission denied. Build with `bun run build`, or set MITOS_TUI to packages/tui/src/app/entries.tsx during development."
        );
    }

    #[test]
    fn tui_nonzero_exit_is_an_error() {
        let mut command = Command::new("mitos-tui");
        let error = run_tui(&mut command, |_| {
            Ok(TuiExit::Failure("exit status: 1".to_string()))
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "Mitos TUI exited with exit status: 1");
    }

    #[test]
    fn tui_successful_exit_is_accepted() {
        let mut command = Command::new("mitos-tui");
        let executed = Cell::new(false);

        run_tui(&mut command, |command| {
            assert_eq!(command.get_program(), "mitos-tui");
            executed.set(true);
            Ok(TuiExit::Success)
        })
        .unwrap();

        assert!(executed.get());
    }
}
