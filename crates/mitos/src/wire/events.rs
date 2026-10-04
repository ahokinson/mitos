use serde::Deserialize;
use serde_json::Value;

use super::responses::CollectedUsage;

pub mod kinds {
    pub const ASSISTANT_DELTA: &str = "assistant_delta";
    pub const ASSISTANT_MESSAGE: &str = "assistant_message";
    pub const TOOL_CALL: &str = "tool_call";
    pub const TOOL_RESULT: &str = "tool_result";
    pub const STATUS: &str = "status";
    pub const USAGE: &str = "usage";
    pub const ERROR: &str = "error";
    pub const NATIVE_SESSION_UPDATE: &str = "native_session_update";
    pub const TURN_COMPLETE: &str = "turn_complete";
    pub const REQUEST: &str = "request";
}

/// A `request` event carries `payload.id` and `payload.kind`; it is answered by
/// `{"action":"answer","request_id":..,"response":..}` on the adapter's stdin.
#[derive(Debug, Default, Deserialize)]
pub struct AdapterEvent {
    pub event: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default)]
    pub usage: Option<CollectedUsage>,
    #[serde(default)]
    pub native_session: Option<Value>,
}

impl AdapterEvent {
    pub fn new(event: &str) -> Self {
        Self {
            event: event.into(),
            ..Self::default()
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.event == kinds::TURN_COMPLETE || self.event == kinds::ERROR
    }
}
