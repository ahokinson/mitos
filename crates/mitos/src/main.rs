mod adapters;
mod cli;
mod config;
mod domain;
mod git;
mod handoff;
mod hooks;
mod ipc;
mod ports;
mod runner;
mod service;
mod store;
mod wire;

fn main() -> anyhow::Result<()> {
    cli::run()
}
