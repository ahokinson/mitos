mod args;
mod hooks;
mod launchers;
mod outputs;
mod threads;
mod views;

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

use crate::adapters::Adapters;
use crate::handoff::DeterministicRenderer;
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

    #[arg(long, global = true, env = "MITOS_ADAPTER_DIR")]
    adapter_dir: Option<PathBuf>,

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
}

pub fn run() -> Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    // A hook runs inside a harness and must not open the database up front.
    if let Some(Command::Hook(args)) = &cli.command {
        return hooks::collect(cli.state_dir, args);
    }
    let tui_state_dir = cli.state_dir.clone();
    let store = Store::new(cli.state_dir)?;
    let renderer = DeterministicRenderer;
    let service = ThreadService::new(&store, &renderer);
    let adapter = Adapters::new(cli.adapter_dir);
    match cli.command {
        None => launchers::launch_tui(tui_state_dir),
        Some(Command::Enter(args)) => threads::enter(&service, &adapter, args),
        Some(Command::Thread { command }) => threads::dispatch(&service, &adapter, command),
        Some(Command::View { command }) => views::dispatch(&service, command),
        Some(Command::Hooks { command }) => hooks::dispatch(&store, command),
        Some(Command::Hook(_)) => unreachable!("handled before the store opens"),
    }
}
