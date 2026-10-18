mod process;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde_json::{Value, json};

use super::clients::ServerClient;
use super::streams::{PendingAsk, SessionStream, agent_for, pending_for, reply_for};
use crate::harnesses::{Emitter, Turn};
use crate::wire::events::{AdapterEvent, kinds};

pub use process::run_turn;

const STREAM_ENDED: &str = "opencode event stream ended before the turn completed";

pub type SharedPending = Arc<Mutex<HashMap<String, PendingAsk>>>;

pub fn answer_handler(
    client: ServerClient,
    pending: SharedPending,
) -> impl FnMut(Value) -> Result<()> + Send + 'static {
    move |answer| {
        if answer.get("action").and_then(Value::as_str) != Some("answer") {
            return Ok(());
        }
        let Some(request_id) = answer.get("request_id").and_then(Value::as_str) else {
            return Ok(());
        };
        let ask = pending
            .lock()
            .expect("opencode pending lock poisoned")
            .remove(request_id);
        let Some(ask) = ask else {
            return Ok(());
        };
        let response = answer.get("response").unwrap_or(&Value::Null);
        let reply = reply_for(&ask, response);
        let _ = client.post(&reply.path, &reply.body);
        Ok(())
    }
}

fn error_event(content: String) -> AdapterEvent {
    AdapterEvent {
        content: Some(content),
        ..AdapterEvent::new(kinds::ERROR)
    }
}

fn run(
    client: &ServerClient,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    let session_id = match turn.session {
        Some(existing) => existing.to_string(),
        None => client.create_session()?,
    };
    let outcome = stream_turn(client, turn, &session_id, emitter, pending);
    if turn.ephemeral {
        let _ = client.delete_session(&session_id);
    }
    outcome
}

fn stream_turn(
    client: &ServerClient,
    turn: &Turn<'_>,
    session_id: &str,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    emitter.emit(AdapterEvent {
        native_session: Some(json!(session_id)),
        ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
    })?;

    // Borrow the account's plan windows once per turn, before prompting;
    // a failed borrow leaves them unset and the estimate covers the gap.
    let plan = super::control::zen_usage(client, session_id);

    // Subscribe before prompting so no early event is missed.
    let events = client.subscribe()?;
    let mut prompt = json!({
        "agent": turn.agent.unwrap_or(agent_for(turn.mode)),
        "parts": [{ "type": "text", "text": turn.text }],
    });
    if let Some(model) = turn.model
        && let Some((provider_id, model_id)) = model.split_once('/')
    {
        prompt["model"] = json!({ "providerID": provider_id, "modelID": model_id });
    }
    client.post(&format!("/session/{session_id}/prompt_async"), &prompt)?;

    let mut stream = SessionStream::new(session_id, plan);
    for raw in events {
        let step = stream.process(&raw);
        for event in step.events {
            if event.event == kinds::REQUEST
                && let Some(ask) = pending_for(&event)
            {
                pending
                    .lock()
                    .expect("opencode pending lock poisoned")
                    .insert(ask.id.clone(), ask);
            }
            emitter.emit(event)?;
        }
        if step.done {
            return Ok(());
        }
    }
    emitter.emit(error_event(STREAM_ENDED.into()))
}

/// Drives one turn against a running server: the caller owns the server
/// process. Emits exactly one terminal event.
pub fn drive_turn(
    client: &ServerClient,
    turn: &Turn<'_>,
    emitter: &mut Emitter<'_, '_>,
    pending: &SharedPending,
) -> Result<()> {
    match run(client, turn, emitter, pending) {
        Ok(()) => Ok(()),
        Err(cause) => emitter.emit(error_event(cause.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::super::clients::ServerClient;
    use super::super::clients::test_support::{
        client_with_connection_error, client_with_responses, http_response,
    };
    use super::*;
    use crate::domain::ThreadMode;

    const SESSION: &str = "ses_fake";

    fn kinds_of(events: &[AdapterEvent]) -> Vec<&str> {
        events.iter().map(|event| event.event.as_str()).collect()
    }

    fn server_event(kind: &str, properties: impl Into<Value>) -> Value {
        let properties = properties.into();
        let mut properties = properties.as_object().unwrap().clone();
        properties.insert("sessionID".into(), json!(SESSION));
        json!({ "type": kind, "properties": properties })
    }

    fn sse(events: impl IntoIterator<Item = Value>) -> String {
        let mut body = String::new();
        for event in events {
            writeln!(body, "data: {event}\n").unwrap();
        }
        http_response("200 OK", "text/event-stream", &body)
    }

    fn responses(
        session: Option<&str>,
        events: impl IntoIterator<Item = Value>,
        replies: usize,
        delete: bool,
    ) -> Vec<String> {
        let mut responses = Vec::new();
        if session.is_none() {
            responses.push(http_response(
                "200 OK",
                "application/json",
                &json!({ "id": SESSION }).to_string(),
            ));
        }
        responses.push(http_response("200 OK", "application/json", "true"));
        responses.push(sse(events));
        responses.push(http_response("204 No Content", "application/json", ""));
        for _ in 0..replies {
            responses.push(http_response("200 OK", "application/json", "true"));
        }
        if delete {
            responses.push(http_response("200 OK", "application/json", "true"));
        }
        responses
    }

    fn completed_events() -> Vec<Value> {
        vec![
            server_event(
                "message.updated",
                json!({ "info": { "id": "msg_a", "role": "assistant", "modelID": "m" } }),
            ),
            server_event(
                "message.part.updated",
                json!({ "part": { "id": "prt_1", "type": "text", "messageID": "msg_a", "text": "done", "time": { "start": 1, "end": 2 } } }),
            ),
            server_event("session.idle", json!({})),
        ]
    }

    fn turn(session: Option<&str>, mode: ThreadMode, ephemeral: bool) -> Turn<'_> {
        Turn {
            workdir: Path::new("/work"),
            text: "hello",
            session,
            mode,
            ephemeral,
            agent: None,
            model: None,
        }
    }

    fn collect(
        client: &ServerClient,
        turn: &Turn<'_>,
        pending: &SharedPending,
        on_event: &mut dyn FnMut(&AdapterEvent) -> Result<()>,
    ) -> Vec<AdapterEvent> {
        let mut seen = Vec::new();
        {
            let mut sink = |event: AdapterEvent| {
                on_event(&event)?;
                seen.push(event);
                Ok(())
            };
            let mut emitter = Emitter::new(&mut sink);
            drive_turn(client, turn, &mut emitter, pending).unwrap();
        }
        seen
    }

    fn recorded(requests: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(requests.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn an_agent_override_replaces_the_mode_default_and_a_model_is_split() {
        let (client, requests) =
            client_with_responses(responses(None, completed_events(), 0, false));
        let mut turn = turn(None, ThreadMode::Build, false);
        turn.agent = Some("review");
        turn.model = Some("opencode/glm-5.3");
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let requests = recorded(&requests);
        assert!(requests.contains(r#"{"agent":"review""#));
        assert!(requests.contains(r#""model":{"providerID":"opencode","modelID":"glm-5.3"}"#));
    }

    #[test]
    fn an_ephemeral_turn_deletes_its_session_afterwards() {
        let (client, requests) =
            client_with_responses(responses(None, completed_events(), 0, true));
        let turn = turn(None, ThreadMode::Plan, true);
        collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert!(recorded(&requests).contains("DELETE /session/ses_fake?directory="));
    }

    #[test]
    fn a_persistent_turn_keeps_its_session() {
        let (client, requests) =
            client_with_responses(responses(None, completed_events(), 0, false));
        let turn = turn(None, ThreadMode::Build, false);
        collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert!(!recorded(&requests).contains("DELETE /session/ses_fake?directory="));
    }

    #[test]
    fn a_new_session_streams_a_turn_and_ends_with_one_turn_complete() {
        let (client, requests) =
            client_with_responses(responses(None, completed_events(), 0, false));
        let turn = turn(None, ThreadMode::Build, false);
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert_eq!(
            kinds_of(&events),
            [
                kinds::NATIVE_SESSION_UPDATE,
                kinds::ASSISTANT_MESSAGE,
                kinds::TURN_COMPLETE
            ]
        );
        assert_eq!(events[0].native_session, Some(json!(SESSION)));
        assert!(
            recorded(&requests)
                .contains(r#"{"agent":"build","parts":[{"type":"text","text":"hello"}]}"#)
        );
    }

    #[test]
    fn an_existing_session_is_resumed_without_creating_one() {
        let events = vec![
            server_event(
                "message.updated",
                json!({ "info": { "id": "msg_a", "role": "assistant" } }),
            ),
            server_event("session.idle", json!({})),
        ];
        let (client, requests) = client_with_responses(responses(Some(SESSION), events, 0, false));
        let turn = turn(Some(SESSION), ThreadMode::Plan, false);
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let requests = recorded(&requests);
        assert!(requests.contains(r#"{"agent":"plan""#));
        assert!(!requests.contains("POST /session?directory="));
    }

    #[test]
    fn a_permission_ask_is_answered_over_the_permission_endpoint() {
        let events = vec![
            server_event(
                "permission.asked",
                json!({ "id": "per_1", "permission": "bash", "patterns": ["ls"] }),
            ),
            server_event("session.idle", json!({})),
        ];
        let (client, requests) = client_with_responses(responses(None, events, 1, false));
        let pending = SharedPending::default();
        let mut answer = answer_handler(client.clone(), pending.clone());
        let turn = turn(None, ThreadMode::Build, false);
        let events = collect(&client, &turn, &pending, &mut |event| {
            if event.event == kinds::REQUEST {
                answer(json!({
                    "action": "answer",
                    "request_id": "per_1",
                    "response": { "allow": true }
                }))?;
            }
            Ok(())
        });

        assert!(kinds_of(&events).contains(&kinds::REQUEST));
        assert_eq!(events.last().unwrap().event, kinds::TURN_COMPLETE);
        let requests = recorded(&requests);
        assert!(requests.contains("POST /permission/per_1/reply?directory="));
        assert!(requests.contains(r#"{"reply":"once"}"#));
    }

    #[test]
    fn a_session_error_ends_the_turn_with_exactly_one_error() {
        let events = vec![server_event(
            "session.error",
            json!({ "error": { "name": "UnknownError", "data": { "message": "boom" } } }),
        )];
        let (client, _) = client_with_responses(responses(None, events, 0, false));
        let turn = turn(None, ThreadMode::Build, false);
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        let terminal: Vec<_> = events
            .iter()
            .filter(|event| event.event == kinds::ERROR || event.event == kinds::TURN_COMPLETE)
            .collect();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].content.as_deref(), Some("boom"));
    }

    #[test]
    fn a_stream_that_closes_early_is_reported_as_an_error() {
        let (client, _) = client_with_responses(responses(None, [], 0, false));
        let turn = turn(None, ThreadMode::Build, false);
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert_eq!(events.last().unwrap().event, kinds::ERROR);
        assert!(
            events
                .last()
                .unwrap()
                .content
                .as_deref()
                .unwrap()
                .contains("event stream ended")
        );
    }

    #[test]
    fn an_unreachable_server_is_reported_as_an_error() {
        let client = client_with_connection_error();
        let turn = turn(None, ThreadMode::Build, false);
        let events = collect(&client, &turn, &SharedPending::default(), &mut |_| Ok(()));

        assert_eq!(kinds_of(&events), [kinds::ERROR]);
        assert!(
            events[0]
                .content
                .as_deref()
                .unwrap()
                .contains("opencode /session failed")
        );
    }
}
