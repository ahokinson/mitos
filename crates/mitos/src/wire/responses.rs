use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::domain::ThreadMode;

#[derive(Clone, Debug, Deserialize)]
pub struct Capabilities {
    pub headless: bool,
    /// Modes the adapter can enforce; empty means build-only.
    #[serde(default)]
    pub modes: Vec<ThreadMode>,
    /// The adapter can raise `request` events and accept `answer` lines on stdin.
    #[serde(default)]
    pub ask_back: bool,
}

#[derive(Debug, Deserialize)]
pub struct NegotiateResponse {
    pub protocol_version: u32,
    pub kind: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Deserialize)]
pub struct LaunchPlan {
    pub protocol_version: u32,
    pub kind: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub native_session: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CollectedHandoff {
    pub protocol_version: u32,
    pub kind: String,
    pub native_session: Option<Value>,
    pub transcript: Option<CollectedTranscript>,
    #[serde(default)]
    pub usage: Option<CollectedUsage>,
}

#[derive(Debug, Deserialize)]
pub struct CollectedTranscript {
    #[serde(default)]
    pub messages: Vec<CollectedMessage>,
}

#[derive(Debug, Deserialize)]
pub struct CollectedMessage {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct CollectedUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub context_used_tokens: Option<u64>,
    #[serde(default)]
    pub context_limit_tokens: Option<u64>,
    pub model: Option<String>,
    pub turns: Option<u32>,
    #[serde(default)]
    pub plan_five_hour_percent: Option<f64>,
    #[serde(default)]
    pub plan_five_hour_resets_at: Option<String>,
    #[serde(default)]
    pub plan_week_percent: Option<f64>,
    #[serde(default)]
    pub plan_week_resets_at: Option<String>,
}
