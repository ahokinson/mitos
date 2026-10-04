mod adapters;
mod cli;
mod config;
mod domain;
mod git;
mod handoff;
mod harnesses;
mod history;
mod hooks;
mod ipc;
mod json;
mod ports;
mod rpc;
mod runner;
mod service;
mod store;
mod wire;

fn main() -> anyhow::Result<()> {
    cli::run()
}
