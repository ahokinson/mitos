use anyhow::Result;

use super::args::DaemonCommand;
use crate::harnesses::opencode::daemon;

pub(super) fn dispatch(command: &DaemonCommand) -> Result<()> {
    match command {
        DaemonCommand::Start => daemon::start(),
        DaemonCommand::Stop => {
            daemon::stop();
            Ok(())
        }
        DaemonCommand::Status { json } => {
            daemon::status(*json);
            Ok(())
        }
        DaemonCommand::Run => daemon::run(),
    }
}
