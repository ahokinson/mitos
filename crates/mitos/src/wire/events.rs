use serde::Deserialize;
use serde_json::Value;

use super::responses::CollectedUsage;

/// A `request` event carries `payload.id` and `payload.kind`; it is answered by
/// `{"action":"answer","request_id":..,"response":..}` on the adapter's stdin.
#[derive(Debug, Deserialize)]
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
