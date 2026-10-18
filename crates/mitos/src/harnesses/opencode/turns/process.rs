use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::super::clients::{ServerAccess, ServerClient};
use super::super::daemon;
use super::super::servers::{ServerStartError, SpawnedServer, spawn_server};
use super::{SharedPending, answer_handler, drive_turn, error_event};
use crate::harnesses::{Emitter, Turn};
use crate::ipc::answers::AnswerListener;

/// Runs one turn: against the shared daemon server when one is available,
/// otherwise over a private `opencode serve` the turn owns and stops.
pub fn run_turn(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    match server_route(daemon::ensure_server(turn.workdir)) {
        ServerRoute::Shared(access) => drive_shared(&access, turn, emitter, answers),
        ServerRoute::Private => run_private(program, turn, emitter, answers),
    }
}

enum ServerRoute {
    Shared(ServerAccess),
    Private,
}

trait PrivateServer {
    fn access(&self) -> &ServerAccess;
    fn stop(&mut self);
}

impl PrivateServer for SpawnedServer {
    fn access(&self) -> &ServerAccess {
        &self.access
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn server_route(access: Option<ServerAccess>) -> ServerRoute {
    match access {
        Some(access) => ServerRoute::Shared(access),
        None => ServerRoute::Private,
    }
}

fn start_private_server<S>(
    started: std::result::Result<S, ServerStartError>,
    emitter: &mut Emitter<'_, '_>,
) -> Result<Option<S>> {
    match started {
        Ok(server) => Ok(Some(server)),
        Err(ServerStartError::Spawn(cause)) => Err(cause),
        Err(ServerStartError::Startup(message)) => {
            emitter.emit(error_event(message))?;
            Ok(None)
        }
    }
}

fn drive_shared(
    access: &ServerAccess,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    let client = ServerClient::new(access, &turn.workdir.to_string_lossy());
    let pending: SharedPending = Arc::default();
    let listener = answers
        .map(|path| AnswerListener::start(path, answer_handler(client.clone(), pending.clone())))
        .transpose();
    match listener {
        Ok(_listener) => drive_turn(&client, turn, emitter, &pending),
        Err(cause) => emitter.emit(error_event(cause.to_string())),
    }
}

fn run_private(
    program: &OsStr,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
) -> Result<()> {
    run_private_with(
        spawn_server(program, turn.workdir),
        turn,
        emitter,
        answers,
        drive_shared,
    )
}

fn run_private_with<S: PrivateServer>(
    started: std::result::Result<S, ServerStartError>,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    answers: Option<&Path>,
    drive: impl FnOnce(&ServerAccess, &Turn<'_>, &mut Emitter<'_, '_>, Option<&Path>) -> Result<()>,
) -> Result<()> {
    let Some(mut spawned) = start_private_server(started, emitter)? else {
        return Ok(());
    };
    let outcome = drive(spawned.access(), turn, emitter, answers);
    spawned.stop();
    outcome
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::domain::ThreadMode;

    struct FakeServer {
        access: ServerAccess,
        stopped: Arc<AtomicBool>,
    }

    impl PrivateServer for FakeServer {
        fn access(&self) -> &ServerAccess {
            &self.access
        }

        fn stop(&mut self) {
            self.stopped.store(true, Ordering::Relaxed);
        }
    }

    fn turn() -> Turn<'static> {
        Turn {
            workdir: Path::new("/work"),
            text: "hello",
            session: None,
            mode: ThreadMode::Build,
            ephemeral: false,
            agent: None,
            model: None,
        }
    }

    #[test]
    fn a_shared_server_is_selected_before_a_private_server() {
        let access = ServerAccess {
            base: "http://shared.test".into(),
            password: "pw".into(),
        };

        daemon::set_test_server(Some(access));
        let route = server_route(daemon::ensure_server(Path::new("/work")));
        daemon::set_test_server(None);
        match route {
            ServerRoute::Shared(access) => assert_eq!(access.base, "http://shared.test"),
            ServerRoute::Private => panic!("the shared server must win"),
        }
        assert!(matches!(server_route(None), ServerRoute::Private));
    }

    #[test]
    fn startup_errors_emit_turn_errors_but_spawn_errors_remain_hard_failures() {
        let mut events = Vec::new();
        let startup = {
            let mut sink = |event| {
                events.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            start_private_server::<SpawnedServer>(
                Err(ServerStartError::Startup("provider missing".into())),
                &mut emitter,
            )
        };
        assert!(startup.unwrap().is_none());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, crate::wire::events::kinds::ERROR);
        assert_eq!(events[0].content.as_deref(), Some("provider missing"));

        let mut sink = |_| Ok(());
        let mut emitter = Emitter::new(&mut sink);
        let result = start_private_server::<SpawnedServer>(
            Err(ServerStartError::Spawn(anyhow::anyhow!("cannot execute"))),
            &mut emitter,
        );
        let Err(error) = result else {
            panic!("a spawn failure must remain a hard error");
        };
        assert_eq!(error.to_string(), "cannot execute");
    }

    #[test]
    fn private_servers_stop_after_the_turn_succeeds_or_fails() {
        for (succeeds, result) in [(true, Ok(())), (false, Err(anyhow::anyhow!("turn failed")))] {
            let stopped = Arc::new(AtomicBool::new(false));
            let server = FakeServer {
                access: ServerAccess {
                    base: "http://private.test".into(),
                    password: "pw".into(),
                },
                stopped: stopped.clone(),
            };
            let mut sink = |_| Ok(());
            let mut emitter = Emitter::new(&mut sink);

            let outcome = run_private_with(
                Ok(server),
                &turn(),
                &mut emitter,
                None,
                |access, _turn, _emitter, _answers| {
                    assert_eq!(access.base, "http://private.test");
                    result
                },
            );

            assert!(stopped.load(Ordering::Relaxed));
            assert_eq!(outcome.is_ok(), succeeds);
        }
    }
}
