use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

use super::tuis::tui_command;
use crate::git::Checkout;

pub fn launch_tui(state_dir: Option<PathBuf>) -> Result<()> {
    let workspace = Checkout::open(std::env::current_dir()?)?;
    let mut command = tui_command()?;
    command.env("MITOS_CORE", std::env::current_exe()?);
    command.env("MITOS_WORKSPACE_ROOT", workspace.root());
    command.env("MITOS_WORKSPACE_KEY", workspace.key());
    if let Some(state_dir) = state_dir {
        command.env("MITOS_STATE_DIR", state_dir);
    }
    let status = command.status().map_err(|error| {
        anyhow!(
            "could not start Mitos TUI: {error}. Build with `bun run build`, or set MITOS_TUI to packages/tui/src/app/entries.tsx during development."
        )
    })?;
    if status.success() {
        Ok(())
    } else {
        bail!("Mitos TUI exited with {status}")
    }
}
