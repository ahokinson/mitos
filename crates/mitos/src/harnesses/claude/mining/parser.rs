use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::harnesses::mining::{
    HOURS_PER_DAY, MinedStats, TokenCounts, day_label, hour_slot, timestamp_ms,
};
use crate::json::{count_value, number_value, object_field, string_value};
use crate::wire::requests::MiningRequest;

/// One session file parsed in a single pass. Cost lines are cumulative, so
/// the latest wins; per-message usage is keyed by message id so a message
/// logged on several lines is counted once.
pub(super) struct SessionParser<'a> {
    request: &'a MiningRequest,
    directory: Option<String>,
    title: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    cost_usd: f64,
    tool_seconds: f64,
    messages_user: u64,
    messages_assistant: u64,
    tool_calls: u64,
    tools: BTreeMap<String, u64>,
    model_costs: BTreeMap<String, f64>,
    message_tokens: BTreeMap<String, TokenCounts>,
    activity: BTreeMap<String, [u64; HOURS_PER_DAY]>,
    first_ms: Option<i64>,
    last_ms: Option<i64>,
    first_user_text: Option<String>,
}

impl<'a> SessionParser<'a> {
    pub(super) fn parse(path: &Path, text: &str, request: &'a MiningRequest) -> Option<MinedStats> {
        let mut parser = Self {
            request,
            directory: None,
            title: None,
            agent: None,
            model: None,
            cost_usd: 0.0,
            tool_seconds: 0.0,
            messages_user: 0,
            messages_assistant: 0,
            tool_calls: 0,
            tools: BTreeMap::new(),
            model_costs: BTreeMap::new(),
            message_tokens: BTreeMap::new(),
            activity: BTreeMap::new(),
            first_ms: None,
            last_ms: None,
            first_user_text: None,
        };
        let mut messages = 0;
        for line in text.split('\n') {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(record) = record.as_object() else {
                continue;
            };
            if parser.absorb(record) {
                messages += 1;
            }
        }
        if messages == 0 {
            return None;
        }
        let first_ms = parser.first_ms.or_else(|| modified_ms(path))?;
        let last_ms = parser.last_ms.unwrap_or(first_ms);
        parser.first_ms = Some(first_ms);
        if !parser.matches_filters() {
            return None;
        }
        let tokens = parser.tokens();
        Some(MinedStats {
            id: path
                .file_stem()
                .map(|id| id.to_string_lossy().into_owned())
                .unwrap_or_default(),
            title: parser.title.or(parser.first_user_text),
            directory: parser.directory.unwrap_or_default(),
            agent: parser.agent,
            model: parser.model,
            day: day_label(first_ms),
            cost_usd: parser.cost_usd,
            tool_seconds: parser.tool_seconds,
            messages_user: parser.messages_user,
            messages_assistant: parser.messages_assistant,
            tool_calls: parser.tool_calls,
            tools: parser.tools.into_iter().collect(),
            model_costs: parser.model_costs.into_iter().collect(),
            tokens,
            activity: parser.activity.into_iter().collect(),
            time_created: first_ms.max(0).cast_unsigned(),
            time_updated: last_ms.max(0).cast_unsigned(),
        })
    }

    /// True when the record is a user or assistant message.
    fn absorb(&mut self, record: &crate::json::Record) -> bool {
        let kind = string_value(record.get("type"));
        match kind {
            Some("user" | "assistant") => {
                let is_message = self.absorb_message(record, kind == Some("assistant"));
                if is_message {
                    if kind == Some("user") {
                        self.messages_user += 1;
                    } else {
                        self.messages_assistant += 1;
                    }
                }
                is_message
            }
            Some("cost-state") => {
                self.absorb_cost_state(record);
                false
            }
            Some("ai-title") => {
                self.title = string_value(record.get("aiTitle")).map(str::to_string);
                false
            }
            Some("agent-name") => {
                self.agent = string_value(record.get("agentName")).map(str::to_string);
                false
            }
            _ => false,
        }
    }

    fn absorb_message(&mut self, record: &crate::json::Record, assistant: bool) -> bool {
        let Some(message) = object_field(Some(record), "message") else {
            return false;
        };
        let content = message.get("content");
        if !assistant && !has_text_blocks(content) {
            return false;
        }
        if self.directory.is_none() {
            self.directory = string_value(record.get("cwd")).map(str::to_string);
        }
        if let Some(model) = string_value(message.get("model")) {
            self.model = Some(model.to_string());
        }
        if let Some(at) = timestamp_ms(record.get("timestamp")) {
            self.note_timestamp(at);
        }
        if let Some(id) = string_value(message.get("id")) {
            self.absorb_usage(id, message.get("usage"));
        }
        if let Some(blocks) = content.and_then(Value::as_array) {
            for block in blocks {
                self.absorb_block(block);
            }
        }
        true
    }

    fn absorb_block(&mut self, block: &Value) {
        let Some(block) = block.as_object() else {
            return;
        };
        match string_value(block.get("type")) {
            Some("text") => {
                if self.first_user_text.is_none() {
                    self.first_user_text = string_value(block.get("text")).map(str::to_string);
                }
            }
            Some("tool_use") => {
                self.tool_calls += 1;
                if let Some(name) = string_value(block.get("name")) {
                    *self.tools.entry(name.to_string()).or_default() += 1;
                }
            }
            _ => {}
        }
    }

    fn absorb_usage(&mut self, id: &str, usage: Option<&Value>) {
        let Some(usage) = usage.and_then(Value::as_object) else {
            return;
        };
        let count = |key: &str| count_value(usage.get(key)).unwrap_or(0);
        let thinking = usage
            .get("output_tokens_details")
            .and_then(Value::as_object)
            .and_then(|details| count_value(details.get("thinking_tokens")))
            .unwrap_or(0);
        self.message_tokens.insert(
            id.to_string(),
            TokenCounts {
                input: count("input_tokens"),
                output: count("output_tokens"),
                reasoning: thinking,
                cache_read: count("cache_read_input_tokens"),
                cache_write: count("cache_creation_input_tokens"),
            },
        );
    }

    fn absorb_cost_state(&mut self, record: &crate::json::Record) {
        self.cost_usd = number_value(record.get("totalCostUSD")).unwrap_or(self.cost_usd);
        self.tool_seconds = number_value(record.get("totalToolDuration"))
            .map_or(self.tool_seconds, |ms| ms / 1000.0);
        if let Some(usage) = object_field(Some(record), "modelUsage") {
            self.model_costs = usage
                .iter()
                .filter_map(|(model, totals)| {
                    let cost = totals
                        .as_object()
                        .and_then(|totals| totals.get("costUSD"))
                        .and_then(|value| number_value(Some(value)));
                    cost.map(|cost| (model.clone(), cost))
                })
                .collect();
        }
    }

    fn note_timestamp(&mut self, at: i64) {
        if let Some(slot) = hour_slot(at) {
            let day = self.activity.entry(day_label(at)).or_default();
            if let Some(count) = day.get_mut(slot.min(HOURS_PER_DAY - 1)) {
                *count += 1;
            }
        }
        self.first_ms = Some(self.first_ms.map_or(at, |first| first.min(at)));
        self.last_ms = Some(self.last_ms.map_or(at, |last| last.max(at)));
    }

    fn matches_filters(&self) -> bool {
        if let Some(directory) = self.request.directory.as_deref()
            && self.directory.as_deref() != Some(directory)
        {
            return false;
        }
        if let Some(since) = self.request.since_ms
            && self.first_ms.is_none_or(|first| first < since)
        {
            return false;
        }
        true
    }

    fn tokens(&self) -> TokenCounts {
        let mut tokens = TokenCounts::default();
        for message in self.message_tokens.values() {
            tokens.input += message.input;
            tokens.output += message.output;
            tokens.reasoning += message.reasoning;
            tokens.cache_read += message.cache_read;
            tokens.cache_write += message.cache_write;
        }
        tokens
    }
}

fn modified_ms(path: &Path) -> Option<i64> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|age| i64::try_from(age.as_millis()).unwrap_or(i64::MAX))
        })
}

/// Text blocks only; a `user` line of `tool_result` blocks is a tool's
/// output, not a message the user wrote.
fn has_text_blocks(content: Option<&Value>) -> bool {
    match content {
        Some(Value::String(_)) => true,
        Some(Value::Array(blocks)) => blocks.iter().any(|block| {
            block
                .as_object()
                .is_some_and(|block| string_value(block.get("type")) == Some("text"))
        }),
        _ => false,
    }
}
