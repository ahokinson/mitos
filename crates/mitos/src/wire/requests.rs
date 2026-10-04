use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::{HarnessKind, ThreadMode, Timestamp};

#[derive(Debug)]
pub struct AdapterRequest {
    pub harness: HarnessKind,
    pub context: String,
    pub native_session: Option<Value>,
}

impl AdapterRequest {
    pub fn prepare_launch(
        harness: HarnessKind,
        context: String,
        native_session: Option<Value>,
    ) -> Self {
        Self {
            harness,
            context,
            native_session,
        }
    }
}

#[derive(Debug)]
pub struct HandoffRequest {
    pub harness: HarnessKind,
    pub workdir: PathBuf,
    pub launched_at: Timestamp,
    pub native_session: Option<Value>,
}

impl HandoffRequest {
    pub fn collect(
        harness: HarnessKind,
        workdir: &Path,
        launched_at: Timestamp,
        native_session: Option<Value>,
    ) -> Self {
        Self {
            harness,
            workdir: workdir.to_path_buf(),
            launched_at,
            native_session,
        }
    }
}

#[derive(Debug)]
pub struct StartThreadRequest {
    pub harness: HarnessKind,
    pub workdir: PathBuf,
    pub mode: ThreadMode,
    pub initial_context: String,
    pub ephemeral: bool,
}

impl StartThreadRequest {
    pub fn new(
        harness: HarnessKind,
        workdir: PathBuf,
        mode: ThreadMode,
        initial_context: String,
    ) -> Self {
        Self {
            harness,
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

#[derive(Debug)]
pub struct AttachThreadRequest {
    pub harness: HarnessKind,
    pub workdir: PathBuf,
    pub native_session: Option<Value>,
}

impl AttachThreadRequest {
    pub fn new(harness: HarnessKind, workdir: PathBuf, native_session: Option<Value>) -> Self {
        Self {
            harness,
            workdir,
            native_session,
        }
    }
}

#[derive(Debug)]
pub struct SendMessageRequest {
    pub harness: HarnessKind,
    pub workdir: PathBuf,
    pub native_session: Option<Value>,
    pub mode: ThreadMode,
    pub text: String,
}

impl SendMessageRequest {
    pub fn new(
        harness: HarnessKind,
        workdir: PathBuf,
        native_session: Option<Value>,
        mode: ThreadMode,
        text: String,
    ) -> Self {
        Self {
            harness,
            workdir,
            native_session,
            mode,
            text,
        }
    }
}

#[derive(Debug)]
pub struct DetachThreadRequest {
    pub harness: HarnessKind,
}

impl DetachThreadRequest {
    pub fn new(harness: HarnessKind) -> Self {
        Self { harness }
    }
}
