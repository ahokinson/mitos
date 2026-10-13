use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::harnesses::mining::{
    HOURS_PER_DAY, MinedStats, TokenCounts, day_label, hour_slot, timestamp_ms,
};
use crate::json::{count_value, object_field, string_value};
use crate::wire::requests::MiningRequest;

/// One rollout file parsed in a single pass. Usage records are per
/// response, so their tokens are summed; message and tool counts come
/// from the response items themselves.
pub(super) struct SessionParser<'a> {
    request: &'a MiningRequest,
    directory: Option<String>,
    id: Option<String>,
    model: Option<String>,
    messages_user: u64,
    messages_assistant: u64,
    tool_calls: u64,
    tools: BTreeMap<String, u64>,
    tokens: TokenCounts,
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
            id: None,
            model: None,
            messages_user: 0,
            messages_assistant: 0,
            tool_calls: 0,
            tools: BTreeMap::new(),
            tokens: TokenCounts::default(),
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
        Some(MinedStats {
            id: parser.id.unwrap_or_else(|| {
                path.file_stem()
                    .map(|id| id.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }),
            title: parser.first_user_text,
            directory: parser.directory.unwrap_or_default(),
            agent: None,
            model: parser.model,
            day: day_label(first_ms),
            cost_usd: 0.0,
            tool_seconds: 0.0,
            messages_user: parser.messages_user,
            messages_assistant: parser.messages_assistant,
            tool_calls: parser.tool_calls,
            tools: parser.tools.into_iter().collect(),
            model_costs: Vec::new(),
            tokens: parser.tokens,
            activity: parser.activity.into_iter().collect(),
            time_created: first_ms.max(0).cast_unsigned(),
            time_updated: last_ms.max(0).cast_unsigned(),
        })
    }

    /// True when the record is a user or assistant message.
    fn absorb(&mut self, record: &crate::json::Record) -> bool {
        let payload = object_field(Some(record), "payload");
        match string_value(record.get("type")) {
            Some("session_meta") => {
                if self.directory.is_none() {
                    self.directory =
                        string_value(payload.and_then(|p| p.get("cwd"))).map(str::to_string);
                }
                if self.id.is_none() {
                    self.id =
                        string_value(payload.and_then(|p| p.get("session_id"))).map(str::to_string);
                }
                false
            }
            Some("turn_context") => {
                if self.directory.is_none() {
                    self.directory =
                        string_value(payload.and_then(|p| p.get("cwd"))).map(str::to_string);
                }
                self.model = string_value(payload.and_then(|p| p.get("model"))).map(str::to_string);
                false
            }
            Some("token_usage_record") => {
                if let Some(usage) = payload
                    .and_then(|p| p.get("usage"))
                    .and_then(Value::as_object)
                {
                    let count = |key: &str| count_value(usage.get(key)).unwrap_or(0);
                    self.tokens.input += count("input_tokens");
                    self.tokens.output += count("output_tokens");
                    self.tokens.reasoning += count("reasoning_output_tokens");
                    self.tokens.cache_read += count("cached_input_tokens");
                    self.tokens.cache_write += count("cache_write_input_tokens");
                }
                false
            }
            Some("response_item") => self.absorb_item(payload, record),
            _ => false,
        }
    }

    fn absorb_item(
        &mut self,
        payload: Option<&crate::json::Record>,
        record: &crate::json::Record,
    ) -> bool {
        let kind = string_value(payload.and_then(|p| p.get("type")));
        match kind {
            Some("message") => {
                let role = string_value(payload.and_then(|p| p.get("role")));
                if role != Some("user") && role != Some("assistant") {
                    return false;
                }
                if role == Some("user") {
                    self.messages_user += 1;
                    self.absorb_user_text(payload);
                } else {
                    self.messages_assistant += 1;
                }
                if let Some(at) = timestamp_ms(record.get("timestamp")) {
                    self.note_timestamp(at);
                }
                true
            }
            Some("function_call" | "custom_tool_call" | "local_shell_call") => {
                self.tool_calls += 1;
                if let Some(name) = string_value(payload.and_then(|p| p.get("name"))) {
                    *self.tools.entry(name.to_string()).or_default() += 1;
                }
                false
            }
            _ => false,
        }
    }

    /// Codex opens a session with machine context wrapped in tags; the
    /// first plain text is the closest thing to a title.
    fn absorb_user_text(&mut self, payload: Option<&crate::json::Record>) {
        if self.first_user_text.is_some() {
            return;
        }
        let Some(blocks) = payload
            .and_then(|p| p.get("content"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for block in blocks {
            let text = block
                .as_object()
                .and_then(|block| string_value(block.get("text")));
            if let Some(text) = text.filter(|text| !text.trim_start().starts_with('<')) {
                self.first_user_text = Some(text.to_string());
                return;
            }
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
