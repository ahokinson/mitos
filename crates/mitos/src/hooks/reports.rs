use serde::Serialize;

use crate::domain::{HarnessKind, Timestamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    /// The harness has no approval step for hooks.
    NotApplicable,
    Trusted,
    /// Installed, but the harness will skip it until the user approves it.
    Untrusted,
}

/// Where one harness's hook stands.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HookStatus {
    pub harness: HarnessKind,
    /// The file Mitos edits, or would create.
    pub target: String,
    pub installed: bool,
    pub trust: Trust,
    /// Why Mitos cannot write the target; `None` when it can.
    pub blocked_by: Option<String>,
    /// Things wrong with the target itself, such as invalid JSON.
    pub problems: Vec<String>,
    pub last_seen: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitResult {
    Installed,
    AlreadyInstalled,
    /// Nothing was changed, and the message says why.
    Skipped,
    /// Mitos will not edit this one; the snippet is for the user to add.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InitOutcome {
    pub harness: String,
    pub result: InitResult,
    pub message: String,
    pub backup: Option<String>,
    pub snippet: Option<String>,
}

impl InitOutcome {
    pub fn new(
        harness: impl std::fmt::Display,
        result: InitResult,
        message: impl Into<String>,
    ) -> Self {
        Self {
            harness: harness.to_string(),
            result,
            message: message.into(),
            backup: None,
            snippet: None,
        }
    }

    pub fn with_backup(mut self, backup: Option<std::path::PathBuf>) -> Self {
        self.backup = backup.map(|path| path.display().to_string());
        self
    }

    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }
}
