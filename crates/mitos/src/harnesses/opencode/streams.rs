use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::domain::{RequestKind, ThreadMode};
use crate::json::{Record, clip, count_value, number_value, string_value};
use crate::wire::events::{AdapterEvent, kinds};
use crate::wire::responses::CollectedUsage;

const ASSISTANT: &str = "assistant";
const TOOL: &str = "tool";

/// Plan runs the native `plan` agent; build runs `build`.
pub fn agent_for(mode: ThreadMode) -> &'static str {
    match mode {
        ThreadMode::Plan => "plan",
        ThreadMode::Build => "build",
    }
}

/// A server-initiated ask awaiting a host decision.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingAsk {
    pub id: String,
    pub kind: RequestKind,
    pub question_count: usize,
}

pub struct AskReply {
    pub path: String,
    pub body: Value,
}

#[derive(Default)]
pub struct StreamStep {
    pub events: Vec<AdapterEvent>,
    pub done: bool,
}

impl StreamStep {
    fn single(event: Option<AdapterEvent>) -> Self {
        Self {
            events: event.into_iter().collect(),
            done: false,
        }
    }
}

fn event(
    kind: &str,
    role: Option<&str>,
    content: Option<String>,
    payload: Option<Value>,
) -> AdapterEvent {
    AdapterEvent {
        role: role.map(str::to_string),
        content,
        payload,
        ..AdapterEvent::new(kind)
    }
}

/// Maps one session's `opencode serve` SSE events onto Mitos adapter events.
/// Reasoning parts are intentionally not surfaced. Exactly one step carries
/// `done`: idle, or the session error.
pub struct SessionStream {
    session_id: String,
    roles: HashMap<String, String>,
    part_types: HashMap<String, String>,
    finished_text: HashSet<String>,
    called_tools: HashSet<String>,
    result_tools: HashSet<String>,
    step_tokens: Vec<(String, Record)>,
    step_costs: Vec<(String, f64)>,
    model: Option<String>,
    started: bool,
}

impl SessionStream {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.into(),
            roles: HashMap::new(),
            part_types: HashMap::new(),
            finished_text: HashSet::new(),
            called_tools: HashSet::new(),
            result_tools: HashSet::new(),
            step_tokens: Vec::new(),
            step_costs: Vec::new(),
            model: None,
            started: false,
        }
    }

    /// Events from other sessions and unknown event types yield nothing.
    pub fn process(&mut self, raw: &Value) -> StreamStep {
        let Some(kind) = string_value(raw.get("type")) else {
            return StreamStep::default();
        };
        let Some(properties) = raw.get("properties").and_then(Value::as_object) else {
            return StreamStep::default();
        };
        if string_value(properties.get("sessionID")) != Some(self.session_id.as_str()) {
            return StreamStep::default();
        }
        match kind {
            "message.updated" => self.message_updated(properties),
            "message.part.updated" => self.part_updated(properties),
            "message.part.delta" => self.part_delta(properties),
            "permission.asked" => {
                self.started = true;
                StreamStep::single(permission_asked(properties))
            }
            "question.asked" => {
                self.started = true;
                StreamStep::single(question_asked(properties))
            }
            "session.status" => self.session_status(properties),
            "session.idle" => self.idle(),
            "session.error" => StreamStep {
                events: vec![event(
                    kinds::ERROR,
                    None,
                    Some(error_message(properties.get("error"))),
                    properties.get("error").cloned(),
                )],
                done: true,
            },
            _ => StreamStep::default(),
        }
    }

    fn message_updated(&mut self, properties: &Record) -> StreamStep {
        self.started = true;
        let info = properties.get("info").and_then(Value::as_object);
        let id = string_value(info.and_then(|info| info.get("id")));
        let role = string_value(info.and_then(|info| info.get("role")));
        if let (Some(id), Some(role)) = (id, role) {
            self.roles.insert(id.into(), role.into());
        }
        if role == Some(ASSISTANT)
            && let Some(model) = string_value(info.and_then(|info| info.get("modelID")))
        {
            self.model = Some(model.into());
        }
        StreamStep::default()
    }

    fn part_delta(&mut self, properties: &Record) -> StreamStep {
        self.started = true;
        if string_value(properties.get("field")) != Some("text") {
            return StreamStep::default();
        }
        let (Some(part_id), Some(message_id), Some(delta)) = (
            string_value(properties.get("partID")),
            string_value(properties.get("messageID")),
            string_value(properties.get("delta")),
        ) else {
            return StreamStep::default();
        };
        if self.roles.get(message_id).map(String::as_str) != Some(ASSISTANT)
            || self.part_types.get(part_id).map(String::as_str) != Some("text")
        {
            return StreamStep::default();
        }
        StreamStep::single(Some(event(
            kinds::ASSISTANT_DELTA,
            Some(ASSISTANT),
            Some(delta.into()),
            None,
        )))
    }

    fn part_updated(&mut self, properties: &Record) -> StreamStep {
        self.started = true;
        let Some(part) = properties.get("part").and_then(Value::as_object) else {
            return StreamStep::default();
        };
        let (Some(id), Some(kind)) = (string_value(part.get("id")), string_value(part.get("type")))
        else {
            return StreamStep::default();
        };
        self.part_types.insert(id.into(), kind.into());
        match kind {
            "text" => StreamStep::single(self.text_finished(id, part)),
            "tool" => StreamStep {
                events: self.tool_events(part),
                done: false,
            },
            "step-finish" => StreamStep::single(self.step_finished(id, part)),
            _ => StreamStep::default(),
        }
    }

    fn text_finished(&mut self, id: &str, part: &Record) -> Option<AdapterEvent> {
        if self.finished_text.contains(id)
            || part.get("synthetic") == Some(&Value::Bool(true))
            || part.get("ignored") == Some(&Value::Bool(true))
        {
            return None;
        }
        let message_id = string_value(part.get("messageID"))?;
        if self.roles.get(message_id).map(String::as_str) != Some(ASSISTANT) {
            return None;
        }
        number_value(part.get("time").and_then(|time| time.get("end")))?;
        let text = string_value(part.get("text"))?;
        self.finished_text.insert(id.into());
        Some(event(
            kinds::ASSISTANT_MESSAGE,
            Some(ASSISTANT),
            Some(text.into()),
            None,
        ))
    }

    fn tool_events(&mut self, part: &Record) -> Vec<AdapterEvent> {
        let call_id = string_value(part.get("callID")).or_else(|| string_value(part.get("id")));
        let state = part.get("state").and_then(Value::as_object);
        let status = string_value(state.and_then(|state| state.get("status")));
        let (Some(call_id), Some(state), Some(status)) = (call_id, state, status) else {
            return Vec::new();
        };
        let settled = status == "completed" || status == "error";
        let mut events = Vec::new();
        if !self.called_tools.contains(call_id) && (status == "running" || settled) {
            self.called_tools.insert(call_id.into());
            events.push(event(
                kinds::TOOL_CALL,
                None,
                Some(describe_tool(part, state)),
                Some(Value::Object(part.clone())),
            ));
        }
        if settled && !self.result_tools.contains(call_id) {
            self.result_tools.insert(call_id.into());
            let content = if status == "completed" {
                string_value(state.get("output"))
            } else {
                string_value(state.get("error"))
            };
            events.push(event(
                kinds::TOOL_RESULT,
                Some(TOOL),
                Some(content.map_or_else(|| describe_tool(part, state), str::to_string)),
                Some(Value::Object(part.clone())),
            ));
        }
        events
    }

    fn step_finished(&mut self, id: &str, part: &Record) -> Option<AdapterEvent> {
        let tokens = part.get("tokens")?.as_object()?;
        upsert(&mut self.step_tokens, id, tokens.clone());
        if let Some(cost) = number_value(part.get("cost")) {
            upsert(&mut self.step_costs, id, cost);
        }
        Some(AdapterEvent {
            usage: Some(self.usage()),
            ..AdapterEvent::new(kinds::USAGE)
        })
    }

    fn usage(&self) -> CollectedUsage {
        let (mut input, mut output, mut cached) = (0, 0, 0);
        for (_, tokens) in &self.step_tokens {
            input += count_value(tokens.get("input")).unwrap_or(0);
            output += count_value(tokens.get("output")).unwrap_or(0);
            cached += cache_read(tokens);
        }
        let last = self.step_tokens.last().map(|(_, tokens)| tokens);
        let last_sum = last.map_or(0, |tokens| {
            count_value(tokens.get("input")).unwrap_or(0)
                + count_value(tokens.get("output")).unwrap_or(0)
                + cache_read(tokens)
        });
        CollectedUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cached_input_tokens: Some(cached),
            cost_usd: (!self.step_costs.is_empty())
                .then(|| self.step_costs.iter().map(|(_, cost)| cost).sum()),
            context_used_tokens: Some(
                count_value(last.and_then(|tokens| tokens.get("total"))).unwrap_or(last_sum),
            ),
            model: self.model.clone(),
            ..CollectedUsage::default()
        }
    }

    fn session_status(&mut self, properties: &Record) -> StreamStep {
        let status = string_value(
            properties
                .get("status")
                .and_then(|status| status.get("type")),
        );
        if status == Some("idle") {
            return self.idle();
        }
        self.started = true;
        StreamStep::default()
    }

    /// Idle before any activity is the session's state from before this turn.
    fn idle(&self) -> StreamStep {
        if !self.started {
            return StreamStep::default();
        }
        StreamStep {
            events: vec![AdapterEvent::new(kinds::TURN_COMPLETE)],
            done: true,
        }
    }
}

/// The pending record for a `request` event the stream produced.
pub fn pending_for(event: &AdapterEvent) -> Option<PendingAsk> {
    let payload = event.payload.as_ref()?;
    let id = string_value(payload.get("id"))?;
    match payload.get("kind").and_then(Value::as_str) {
        Some("permission") => Some(PendingAsk {
            id: id.into(),
            kind: RequestKind::Permission,
            question_count: 0,
        }),
        Some("question") => {
            let count = payload
                .pointer("/params/questions")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            Some(PendingAsk {
                id: id.into(),
                kind: RequestKind::Question,
                question_count: count.max(1),
            })
        }
        _ => None,
    }
}

/// The HTTP reply answering `pending`, from the response shapes the TUI sends:
/// `{allow}` for permissions, `{answer}` for questions.
pub fn reply_for(pending: &PendingAsk, response: &Value) -> AskReply {
    if pending.kind == RequestKind::Question {
        let text = string_value(response.get("answer")).unwrap_or("");
        let answers: Vec<Vec<&str>> = (0..pending.question_count).map(|_| vec![text]).collect();
        return AskReply {
            path: format!("/question/{}/reply", pending.id),
            body: json!({ "answers": answers }),
        };
    }
    let allowed = response.get("allow") == Some(&Value::Bool(true));
    AskReply {
        path: format!("/permission/{}/reply", pending.id),
        body: json!({ "reply": if allowed { "once" } else { "reject" } }),
    }
}

fn upsert<T>(entries: &mut Vec<(String, T)>, key: &str, value: T) {
    match entries.iter_mut().find(|(existing, _)| existing == key) {
        Some(entry) => entry.1 = value,
        None => entries.push((key.into(), value)),
    }
}

fn cache_read(tokens: &Record) -> u64 {
    count_value(tokens.get("cache").and_then(|cache| cache.get("read"))).unwrap_or(0)
}

fn permission_asked(properties: &Record) -> Option<AdapterEvent> {
    let id = string_value(properties.get("id"))?;
    let permission = string_value(properties.get("permission")).unwrap_or("permission");
    let patterns: Vec<&str> = properties
        .get("patterns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let title = if patterns.is_empty() {
        clip(permission)
    } else {
        clip(&format!("{permission}: {}", patterns.join(", ")))
    };
    Some(request(id, RequestKind::Permission, &title, properties))
}

fn question_asked(properties: &Record) -> Option<AdapterEvent> {
    let id = string_value(properties.get("id"))?;
    let first = properties
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(Value::as_object);
    let options: Vec<&str> = first
        .and_then(|first| first.get("options"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| string_value(option.get("label")))
        .collect();
    let text = string_value(first.and_then(|first| first.get("question"))).unwrap_or("question");
    let title = if options.is_empty() {
        clip(text)
    } else {
        clip(&format!("{text} [{}]", options.join(" / ")))
    };
    Some(request(id, RequestKind::Question, &title, properties))
}

fn request(id: &str, kind: RequestKind, title: &str, params: &Record) -> AdapterEvent {
    AdapterEvent {
        payload: Some(json!({
            "id": id,
            "kind": kind.as_str(),
            "title": title,
            "params": params,
        })),
        ..AdapterEvent::new(kinds::REQUEST)
    }
}

fn error_message(error: Option<&Value>) -> String {
    let data = error.and_then(|error| error.get("data"));
    string_value(data.and_then(|data| data.get("message")))
        .or_else(|| string_value(error.and_then(|error| error.get("message"))))
        .or_else(|| string_value(error.and_then(|error| error.get("name"))))
        .unwrap_or("OpenCode reported an error")
        .to_string()
}

fn describe_tool(part: &Record, state: &Record) -> String {
    let tool = string_value(part.get("tool")).unwrap_or("tool");
    if let Some(title) = string_value(state.get("title")) {
        return clip(&format!("{tool}: {title}"));
    }
    let input = state.get("input");
    let detail = ["command", "filePath", "pattern", "url"]
        .iter()
        .find_map(|key| string_value(input.and_then(|input| input.get(*key))));
    match detail {
        Some(detail) => clip(&format!("{tool}: {detail}")),
        None => clip(tool),
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use super::*;
    use crate::harnesses::fixtures::kinds_of;

    const SESSION: &str = "ses_1";

    fn event_of(kind: &str, properties: Value) -> Value {
        let mut properties = properties.as_object().unwrap().clone();
        properties.insert("sessionID".into(), json!(SESSION));
        json!({ "type": kind, "properties": properties })
    }

    fn assistant_message(stream: &mut SessionStream) {
        stream.process(&event_of(
            "message.updated",
            json!({ "info": { "id": "msg_a", "role": "assistant", "modelID": "glm-5.3" } }),
        ));
    }

    #[test]
    fn plan_mode_runs_the_plan_agent_and_build_runs_build() {
        assert_eq!(agent_for(ThreadMode::Plan), "plan");
        assert_eq!(agent_for(ThreadMode::Build), "build");
    }

    #[test]
    fn events_from_other_sessions_are_ignored() {
        let step = SessionStream::new(SESSION).process(&json!({
            "type": "session.idle",
            "properties": { "sessionID": "ses_other" }
        }));
        assert!(step.events.is_empty() && !step.done);
    }

    #[test]
    fn assistant_text_deltas_stream_and_the_finished_part_is_one_message() {
        let mut stream = SessionStream::new(SESSION);
        assistant_message(&mut stream);
        stream.process(&event_of(
            "message.part.updated",
            json!({ "part": { "id": "prt_1", "type": "text", "messageID": "msg_a", "text": "" } }),
        ));
        let delta = stream.process(&event_of(
            "message.part.delta",
            json!({ "messageID": "msg_a", "partID": "prt_1", "field": "text", "delta": "Hel" }),
        ));
        assert_eq!(kinds_of(&delta.events), [kinds::ASSISTANT_DELTA]);
        assert_eq!(delta.events[0].role.as_deref(), Some("assistant"));
        assert_eq!(delta.events[0].content.as_deref(), Some("Hel"));

        let part = json!({
            "id": "prt_1", "type": "text", "messageID": "msg_a", "text": "Hello",
            "time": { "start": 1, "end": 2 }
        });
        let done = stream.process(&event_of("message.part.updated", json!({ "part": part })));
        assert_eq!(kinds_of(&done.events), [kinds::ASSISTANT_MESSAGE]);
        assert_eq!(done.events[0].content.as_deref(), Some("Hello"));
        let repeat = stream.process(&event_of("message.part.updated", json!({ "part": part })));
        assert!(repeat.events.is_empty());
    }

    #[test]
    fn user_text_and_unfinished_text_produce_nothing() {
        let mut stream = SessionStream::new(SESSION);
        stream.process(&event_of(
            "message.updated",
            json!({ "info": { "id": "msg_u", "role": "user" } }),
        ));
        let user = stream.process(&event_of(
            "message.part.updated",
            json!({ "part": { "id": "prt_u", "type": "text", "messageID": "msg_u", "text": "hi", "time": { "start": 1, "end": 2 } } }),
        ));
        assert!(user.events.is_empty());
        assistant_message(&mut stream);
        let unfinished = stream.process(&event_of(
            "message.part.updated",
            json!({ "part": { "id": "prt_a", "type": "text", "messageID": "msg_a", "text": "par", "time": { "start": 1 } } }),
        ));
        assert!(unfinished.events.is_empty());
    }

    #[test]
    fn a_tool_part_yields_one_call_and_one_result() {
        let mut stream = SessionStream::new(SESSION);
        let mut part = |state: Value| {
            stream.process(&event_of(
                "message.part.updated",
                json!({ "part": { "id": "prt_t", "type": "tool", "callID": "call_1", "tool": "bash", "state": state } }),
            ))
        };
        assert!(
            part(json!({ "status": "pending", "input": {}, "raw": "" }))
                .events
                .is_empty()
        );
        let running = part(
            json!({ "status": "running", "input": { "command": "ls" }, "time": { "start": 1 } }),
        );
        assert_eq!(kinds_of(&running.events), [kinds::TOOL_CALL]);
        assert_eq!(running.events[0].content.as_deref(), Some("bash: ls"));
        let completed = part(
            json!({ "status": "completed", "input": { "command": "ls" }, "output": "a\nb", "time": { "start": 1, "end": 2 } }),
        );
        assert_eq!(kinds_of(&completed.events), [kinds::TOOL_RESULT]);
        assert_eq!(completed.events[0].content.as_deref(), Some("a\nb"));
        let repeat = part(
            json!({ "status": "completed", "input": {}, "output": "a\nb", "time": { "start": 1, "end": 2 } }),
        );
        assert!(repeat.events.is_empty());
    }

    #[test]
    fn a_failed_tool_reports_its_error_as_the_result() {
        let step = SessionStream::new(SESSION).process(&event_of(
            "message.part.updated",
            json!({ "part": {
                "id": "prt_t", "type": "tool", "callID": "call_1", "tool": "edit",
                "state": { "status": "error", "input": { "filePath": "a.ts" }, "error": "no such file", "time": { "start": 1, "end": 2 } }
            } }),
        ));
        assert_eq!(
            kinds_of(&step.events),
            [kinds::TOOL_CALL, kinds::TOOL_RESULT]
        );
        assert_eq!(step.events[0].content.as_deref(), Some("edit: a.ts"));
        assert_eq!(step.events[1].content.as_deref(), Some("no such file"));
    }

    fn finish(stream: &mut SessionStream, id: &str, tokens: Value) -> StreamStep {
        stream.process(&event_of(
            "message.part.updated",
            json!({ "part": { "id": id, "type": "step-finish", "messageID": "msg_a", "tokens": tokens } }),
        ))
    }

    #[test]
    fn step_finish_accumulates_usage_across_steps() {
        let mut stream = SessionStream::new(SESSION);
        assistant_message(&mut stream);
        finish(
            &mut stream,
            "prt_s1",
            json!({ "total": 150, "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 10, "write": 0 } }),
        );
        let second = finish(
            &mut stream,
            "prt_s2",
            json!({ "total": 260, "input": 200, "output": 60, "reasoning": 0, "cache": { "read": 20, "write": 0 } }),
        );
        let usage = second.events[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_input_tokens
            ),
            (Some(300), Some(110), Some(30))
        );
        assert_eq!(usage.context_used_tokens, Some(260));
        assert_eq!(usage.model.as_deref(), Some("glm-5.3"));
        assert_eq!(usage.cost_usd, None);
        let repeat = finish(
            &mut stream,
            "prt_s2",
            json!({ "total": 260, "input": 200, "output": 60, "reasoning": 0, "cache": { "read": 20, "write": 0 } }),
        );
        assert_eq!(
            repeat.events[0].usage.as_ref().unwrap().input_tokens,
            Some(300)
        );
    }

    #[test]
    fn context_falls_back_to_the_last_steps_token_sum_without_a_total() {
        let mut stream = SessionStream::new(SESSION);
        let step = finish(
            &mut stream,
            "prt_s1",
            json!({ "input": 5, "output": 2, "cache": { "read": 3 } }),
        );
        assert_eq!(
            step.events[0].usage.as_ref().unwrap().context_used_tokens,
            Some(10)
        );
    }

    #[test]
    fn step_finish_costs_sum_across_steps_without_double_counting_a_repeat() {
        let mut stream = SessionStream::new(SESSION);
        assistant_message(&mut stream);
        let mut cost_step = |id: &str, cost: Value| {
            let mut part = json!({ "id": id, "type": "step-finish", "messageID": "msg_a",
                "tokens": { "input": 1, "output": 1, "reasoning": 0, "cache": { "read": 0, "write": 0 } } });
            if !cost.is_null() {
                part["cost"] = cost;
            }
            stream.process(&event_of("message.part.updated", json!({ "part": part })))
        };
        assert_eq!(
            cost_step("prt_s0", Value::Null).events[0]
                .usage
                .as_ref()
                .unwrap()
                .cost_usd,
            None
        );
        cost_step("prt_s1", json!(0.25));
        let total = |step: StreamStep| step.events[0].usage.as_ref().unwrap().cost_usd.unwrap();
        assert!((total(cost_step("prt_s2", json!(0.5))) - 0.75).abs() < 1e-9);
        assert!((total(cost_step("prt_s2", json!(0.5))) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn idle_before_any_activity_is_not_completion() {
        assert!(
            !SessionStream::new(SESSION)
                .process(&event_of("session.idle", json!({})))
                .done
        );
    }

    #[test]
    fn idle_after_activity_completes_the_turn() {
        let mut stream = SessionStream::new(SESSION);
        assistant_message(&mut stream);
        let step = stream.process(&event_of("session.idle", json!({})));
        assert!(step.done);
        assert_eq!(kinds_of(&step.events), [kinds::TURN_COMPLETE]);
    }

    #[test]
    fn an_ask_counts_as_activity_so_a_later_idle_completes_the_turn() {
        let mut stream = SessionStream::new(SESSION);
        stream.process(&event_of(
            "permission.asked",
            json!({ "id": "per_1", "permission": "bash" }),
        ));
        assert!(stream.process(&event_of("session.idle", json!({}))).done);
    }

    #[test]
    fn a_session_status_of_idle_completes_like_session_idle() {
        let mut stream = SessionStream::new(SESSION);
        stream.process(&event_of(
            "session.status",
            json!({ "status": { "type": "busy" } }),
        ));
        let step = stream.process(&event_of(
            "session.status",
            json!({ "status": { "type": "idle" } }),
        ));
        assert!(step.done);
        assert_eq!(step.events[0].event, kinds::TURN_COMPLETE);
    }

    #[test]
    fn a_session_error_is_the_terminal_event() {
        let step = SessionStream::new(SESSION).process(&event_of(
            "session.error",
            json!({ "error": { "name": "ProviderAuthError", "data": { "message": "bad key" } } }),
        ));
        assert!(step.done);
        assert_eq!(step.events[0].event, kinds::ERROR);
        assert_eq!(step.events[0].content.as_deref(), Some("bad key"));
    }

    #[test]
    fn a_permission_ask_becomes_a_permission_request_and_maps_to_a_reply() {
        let mut stream = SessionStream::new(SESSION);
        let step = stream.process(&event_of(
            "permission.asked",
            json!({ "id": "per_1", "permission": "bash", "patterns": ["rm -rf build"] }),
        ));
        let request = &step.events[0];
        assert_eq!(request.event, kinds::REQUEST);
        let payload = request.payload.as_ref().unwrap();
        assert_eq!(payload["id"], "per_1");
        assert_eq!(payload["kind"], "permission");
        assert_eq!(payload["title"], "bash: rm -rf build");
        let pending = pending_for(request).unwrap();
        assert_eq!(
            pending,
            PendingAsk {
                id: "per_1".into(),
                kind: RequestKind::Permission,
                question_count: 0
            }
        );
        let allow = reply_for(&pending, &json!({ "allow": true }));
        assert_eq!(allow.path, "/permission/per_1/reply");
        assert_eq!(allow.body, json!({ "reply": "once" }));
        assert_eq!(
            reply_for(&pending, &json!({ "allow": false })).body,
            json!({ "reply": "reject" })
        );
    }

    #[test]
    fn a_question_ask_lists_options_and_answers_every_question() {
        let mut stream = SessionStream::new(SESSION);
        let step = stream.process(&event_of(
            "question.asked",
            json!({ "id": "que_1", "questions": [
                { "question": "Which db?", "header": "DB", "options": [
                    { "label": "pg", "description": "" }, { "label": "sqlite", "description": "" }
                ] },
                { "question": "Why?", "header": "Why", "options": [] }
            ] }),
        ));
        let request = &step.events[0];
        let payload = request.payload.as_ref().unwrap();
        assert_eq!(payload["kind"], "question");
        assert_eq!(payload["title"], "Which db? [pg / sqlite]");
        let pending = pending_for(request).unwrap();
        assert_eq!(pending.question_count, 2);
        let reply = reply_for(&pending, &json!({ "answer": "pg" }));
        assert_eq!(reply.path, "/question/que_1/reply");
        assert_eq!(reply.body, json!({ "answers": [["pg"], ["pg"]] }));
    }
}
