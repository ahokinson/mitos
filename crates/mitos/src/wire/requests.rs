use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::versions::PROTOCOL_VERSION;
use crate::domain::ThreadMode;

#[derive(Debug, Serialize)]
pub struct NegotiateRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
}

impl NegotiateRequest {
    pub fn new(harness: &str) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "negotiate",
            harness: harness.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdapterRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub workdir: PathBuf,
    pub context: String,
    pub native_session: Option<Value>,
}

impl AdapterRequest {
    pub fn prepare_launch(
        harness: &str,
        workdir: &Path,
        context: String,
        native_session: Option<Value>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "prepare_launch",
            harness: harness.into(),
            workdir: workdir.to_path_buf(),
            context,
            native_session,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HandoffRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub workdir: PathBuf,
    pub launched_at: String,
    pub native_session: Option<Value>,
}

impl HandoffRequest {
    pub fn collect(
        harness: &str,
        workdir: &Path,
        launched_at: String,
        native_session: Option<Value>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "collect_handoff",
            harness: harness.into(),
            workdir: workdir.to_path_buf(),
            launched_at,
            native_session,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StartThreadRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub thread_id: String,
    pub workdir: PathBuf,
    pub mode: ThreadMode,
    pub initial_context: String,
    pub ephemeral: bool,
}

impl StartThreadRequest {
    pub fn new(
        harness: &str,
        thread_id: &str,
        workdir: PathBuf,
        mode: ThreadMode,
        initial_context: String,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "start_thread",
            harness: harness.into(),
            thread_id: thread_id.into(),
            workdir,
            mode,
            initial_context,
            ephemeral: false,
        }
    }

    pub fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }
}

#[derive(Debug, Serialize)]
pub struct AttachThreadRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub thread_id: String,
    pub workdir: PathBuf,
    pub native_session: Option<Value>,
    pub since_cursor: i64,
}

impl AttachThreadRequest {
    pub fn new(
        harness: &str,
        thread_id: &str,
        workdir: PathBuf,
        native_session: Option<Value>,
        since_cursor: i64,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "attach_thread",
            harness: harness.into(),
            thread_id: thread_id.into(),
            workdir,
            native_session,
            since_cursor,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub thread_id: String,
    pub workdir: PathBuf,
    pub native_session: Option<Value>,
    pub mode: ThreadMode,
    pub turn_id: String,
    pub text: String,
}

impl SendMessageRequest {
    pub fn new(
        harness: &str,
        thread_id: &str,
        workdir: PathBuf,
        native_session: Option<Value>,
        mode: ThreadMode,
        turn_id: &str,
        text: String,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "send_message",
            harness: harness.into(),
            thread_id: thread_id.into(),
            workdir,
            native_session,
            mode,
            turn_id: turn_id.into(),
            text,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DetachThreadRequest {
    pub protocol_version: u32,
    pub action: &'static str,
    pub harness: String,
    pub native_session: Option<Value>,
}

impl DetachThreadRequest {
    pub fn new(harness: &str, native_session: Option<Value>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            action: "detach_thread",
            harness: harness.into(),
            native_session,
        }
    }
}
