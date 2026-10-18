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

#[cfg(test)]
trait DaemonOperations {
    fn start(&self) -> Result<()>;
    fn stop(&self);
    fn status(&self, json: bool);
    fn run(&self) -> Result<()>;
}

#[cfg(test)]
fn dispatch_with(command: &DaemonCommand, operations: &impl DaemonOperations) -> Result<()> {
    match command {
        DaemonCommand::Start => operations.start(),
        DaemonCommand::Stop => {
            operations.stop();
            Ok(())
        }
        DaemonCommand::Status { json } => {
            operations.status(*json);
            Ok(())
        }
        DaemonCommand::Run => operations.run(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[derive(Default)]
    struct FakeOperations {
        operation: Cell<Option<&'static str>>,
        json: Cell<Option<bool>>,
        start_fails: bool,
    }

    impl DaemonOperations for FakeOperations {
        fn start(&self) -> Result<()> {
            self.operation.set(Some("start"));
            if self.start_fails {
                anyhow::bail!("daemon unavailable");
            }
            Ok(())
        }

        fn stop(&self) {
            self.operation.set(Some("stop"));
        }

        fn status(&self, json: bool) {
            self.operation.set(Some("status"));
            self.json.set(Some(json));
        }

        fn run(&self) -> Result<()> {
            self.operation.set(Some("run"));
            Ok(())
        }
    }

    #[test]
    fn start_propagates_the_daemon_error() {
        let operations = FakeOperations {
            start_fails: true,
            ..FakeOperations::default()
        };
        let error = dispatch_with(&DaemonCommand::Start, &operations).unwrap_err();

        assert_eq!(error.to_string(), "daemon unavailable");
        assert_eq!(operations.operation.get(), Some("start"));
    }

    #[test]
    fn dispatches_every_successful_operation() {
        let operations = FakeOperations::default();

        dispatch_with(&DaemonCommand::Start, &operations).unwrap();
        assert_eq!(operations.operation.get(), Some("start"));

        dispatch_with(&DaemonCommand::Stop, &operations).unwrap();
        assert_eq!(operations.operation.get(), Some("stop"));

        dispatch_with(&DaemonCommand::Status { json: true }, &operations).unwrap();
        assert_eq!(operations.operation.get(), Some("status"));
        assert_eq!(operations.json.get(), Some(true));

        dispatch_with(&DaemonCommand::Run, &operations).unwrap();
        assert_eq!(operations.operation.get(), Some("run"));
    }
}
