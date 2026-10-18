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

/// How many entries a merged mining report keeps per series and list.
pub const DEFAULT_TOP: i64 = 10;

const MS_PER_DAY: i64 = 86_400_000;

/// Filters for a mining pass over a harness's stored history; providers that
/// cannot honor a filter ignore it rather than fail.
#[derive(Debug)]
pub struct MiningRequest {
    pub directory: Option<String>,
    pub since_ms: Option<i64>,
    pub top: i64,
}

impl MiningRequest {
    /// `since_days` counts back from now; `None` reads the whole history.
    pub fn new(directory: Option<String>, since_days: Option<i64>, top: i64) -> Self {
        let since_ms =
            since_days.map(|days| chrono::Utc::now().timestamp_millis() - days * MS_PER_DAY);
        Self {
            directory,
            since_ms,
            top: top.max(1),
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
    /// A harness agent override; `None` follows the mode's default agent.
    pub agent: Option<String>,
    /// A model reference, `provider/model`; `None` follows the harness default.
    pub model: Option<String>,
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
            agent: None,
            model: None,
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
    /// A harness agent override; `None` follows the mode's default agent.
    pub agent: Option<String>,
    /// A model reference, `provider/model`; `None` follows the harness default.
    pub model: Option<String>,
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
            agent: None,
            model: None,
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
