mod args;
mod daemons;
mod hooks;
mod launchers;
mod outputs;
mod threads;
mod tuis;
mod views;

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

use crate::handoff::DeterministicRenderer;
use crate::harnesses::Adapters;
use crate::service::ThreadService;
use crate::store::Store;

#[derive(Debug, Parser)]
#[command(
    name = "mitos",
    version,
    about = "Keep the thread of work across native coding agents"
)]
struct Cli {
    #[arg(long, global = true, env = "MITOS_STATE_DIR")]
    state_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Enter a native harness through its adapter (PTY-fallback escape hatch).
    Enter(args::EnterArgs),
    /// Manage threads.
    Thread {
        #[command(subcommand)]
        command: args::ThreadCommand,
    },
    /// Read-only views of stored state, as JSON, for frontends.
    View {
        #[command(subcommand)]
        command: args::ViewCommand,
    },
    /// Record a harness hook payload from stdin; install it with `mitos hooks init`.
    Hook(args::HookArgs),
    /// Install and inspect the harness hooks that feed Mitos.
    Hooks {
        #[command(subcommand)]
        command: args::HooksCommand,
    },
    /// Manage the shared `OpenCode` server that turns connect to.
    Daemon {
        #[command(subcommand)]
        command: args::DaemonCommand,
    },
}

/// # Errors
/// Returns the failure of the dispatched command.
pub fn run() -> Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    if let Some(state_dir) = &cli.state_dir {
        // Keep every later reader — the store, the daemon, spawned children —
        // on one state root, flag or environment alike.
        //
        // SAFETY: no other thread exists yet.
        unsafe { std::env::set_var("MITOS_STATE_DIR", state_dir) };
    }
    // A hook runs inside a harness and must not open the database up front.
    if let Some(Command::Hook(args)) = &cli.command {
        return hooks::collect(cli.state_dir.clone(), args);
    }
    // The daemon owns files and processes only; it must not open the database.
    if let Some(Command::Daemon { command }) = &cli.command {
        return daemons::dispatch(command);
    }
    let tui_state_dir = cli.state_dir.clone();
    let store = Store::new(cli.state_dir)?;
    let renderer = DeterministicRenderer;
    let service = ThreadService::new(&store, &renderer);
    let adapter = Adapters;
    match cli.command {
        None => launchers::launch_tui(tui_state_dir.as_deref()),
        Some(Command::Enter(args)) => threads::enter(&service, &adapter, args),
        Some(Command::Thread { command }) => threads::dispatch(&service, &adapter, command),
        Some(Command::View { command }) => views::dispatch(&service, command),
        Some(Command::Hooks { command }) => hooks::dispatch(&store, command),
        Some(Command::Hook(_) | Command::Daemon { .. }) => {
            unreachable!("handled before the store opens")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_a_state_directory_and_daemon_status_json() {
        let cli = Cli::try_parse_from([
            "mitos",
            "--state-dir",
            "/var/lib/mitos",
            "daemon",
            "status",
            "--json",
        ])
        .unwrap();

        assert_eq!(cli.state_dir, Some(PathBuf::from("/var/lib/mitos")));
        assert!(matches!(
            cli.command,
            Some(Command::Daemon {
                command: args::DaemonCommand::Status { json: true }
            })
        ));
    }

    #[test]
    fn parser_defaults_to_launching_the_tui() {
        let cli = Cli::try_parse_from(["mitos"]).unwrap();

        assert!(cli.state_dir.is_none());
        assert!(cli.command.is_none());
    }
}
