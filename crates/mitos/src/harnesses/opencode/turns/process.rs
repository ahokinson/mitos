use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::super::clients::{ServerAccess, ServerClient};
use super::super::daemon;
use super::super::servers::{ServerStartError, spawn_server};
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
    if let Some(access) = daemon::ensure_server(turn.workdir) {
        return drive_shared(&access, turn, emitter, answers);
    }
    run_private(program, turn, emitter, answers)
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
    // A harness that cannot even start is a hard failure for the caller; one
    // that starts but dies reports itself as a turn error.
    let mut spawned = match spawn_server(program, turn.workdir) {
        Ok(spawned) => spawned,
        Err(ServerStartError::Spawn(cause)) => return Err(cause),
        Err(ServerStartError::Startup(message)) => return emitter.emit(error_event(message)),
    };
    let client = ServerClient::new(&spawned.access, &turn.workdir.to_string_lossy());
    let pending: SharedPending = Arc::default();
    let listener = answers
        .map(|path| AnswerListener::start(path, answer_handler(client.clone(), pending.clone())))
        .transpose();
    let outcome = match listener {
        Ok(_listener) => drive_turn(&client, turn, emitter, &pending),
        Err(cause) => emitter.emit(error_event(cause.to_string())),
    };
    let _ = spawned.child.kill();
    let _ = spawned.child.wait();
    outcome
}
