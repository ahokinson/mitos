use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use super::ThreadService;
use crate::domain::{EventKind, RequestStatus, ThreadMode, ThreadStatus, id};
use crate::handoff::DeterministicRenderer;
use crate::ports::{EventSink, HarnessAdapter};
use crate::store::Store;
use crate::wire::events::AdapterEvent;
use crate::wire::requests::{
    AdapterRequest, AttachThreadRequest, DetachThreadRequest, HandoffRequest, SendMessageRequest,
    StartThreadRequest,
};
use crate::wire::responses::{Capabilities, CollectedHandoff, LaunchPlan};

struct FakeAdapter {
    capabilities: Value,
    start_events: Vec<Value>,
    start_session: Option<Value>,
    send_events: Vec<Value>,
    handoff: Option<Value>,
    calls: RefCell<Vec<&'static str>>,
    sent: RefCell<Vec<(Option<Value>, String)>>,
    started_with: RefCell<Vec<String>>,
}

impl FakeAdapter {
    fn new() -> Self {
        Self {
            capabilities: json!({"headless": true, "modes": ["plan"]}),
            start_events: Vec::new(),
            start_session: None,
            send_events: Vec::new(),
            handoff: None,
            calls: RefCell::new(Vec::new()),
            sent: RefCell::new(Vec::new()),
            started_with: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.borrow().clone()
    }
}

fn emit(events: &[Value], on_event: &mut EventSink<'_>) -> Result<()> {
    for event in events {
        on_event(serde_json::from_value::<AdapterEvent>(event.clone())?)?;
    }
    Ok(())
}

impl HarnessAdapter for FakeAdapter {
    fn negotiate(&self, _harness: &str) -> Result<Capabilities> {
        self.calls.borrow_mut().push("negotiate");
        Ok(serde_json::from_value(self.capabilities.clone())?)
    }

    fn prepare_launch(&self, _request: &AdapterRequest) -> Result<LaunchPlan> {
        Err(anyhow!("prepare_launch is not used by the service"))
    }

    fn collect_handoff(&self, _request: &HandoffRequest) -> Result<CollectedHandoff> {
        self.calls.borrow_mut().push("collect_handoff");
        match &self.handoff {
            Some(handoff) => Ok(serde_json::from_value::<CollectedHandoff>(handoff.clone())?),
            None => Err(anyhow!("collection failed")),
        }
    }

    fn start_thread(
        &self,
        request: &StartThreadRequest,
        on_event: &mut EventSink<'_>,
        _answers: Option<&Path>,
    ) -> Result<Option<Value>> {
        self.calls.borrow_mut().push("start_thread");
        self.started_with
            .borrow_mut()
            .push(request.initial_context.clone());
        emit(&self.start_events, on_event)?;
        Ok(self.start_session.clone())
    }

    fn attach_thread(
        &self,
        _request: &AttachThreadRequest,
        _on_event: &mut EventSink<'_>,
    ) -> Result<()> {
        self.calls.borrow_mut().push("attach_thread");
        Ok(())
    }

    fn send_message(
        &self,
        request: &SendMessageRequest,
        on_event: &mut EventSink<'_>,
        _answers: Option<&Path>,
    ) -> Result<()> {
        self.calls.borrow_mut().push("send_message");
        self.sent
            .borrow_mut()
            .push((request.native_session.clone(), request.text.clone()));
        emit(&self.send_events, on_event)
    }

    fn detach_thread(&self, _request: &DetachThreadRequest) -> Result<()> {
        self.calls.borrow_mut().push("detach_thread");
        Ok(())
    }
}

struct Fixture {
    store: Store,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("mitos-service-test-{}", id()));
        Self {
            store: Store::new(Some(root.clone())).unwrap(),
            root,
        }
    }

    fn service(&self) -> ThreadService<'_> {
        ThreadService::new(&self.store, &DeterministicRenderer)
    }

    fn thread(&self, harness: Option<&str>) -> String {
        let workspace = self
            .store
            .workspace_for(Path::new("/workspace"), None, "key")
            .unwrap();
        let thread = self.store.create_thread(&workspace.id).unwrap();
        if let Some(harness) = harness {
            self.service()
                .bind_harness(&thread.id, harness, None)
                .unwrap();
        }
        thread.id
    }

    fn kinds(&self, thread_id: &str) -> Vec<EventKind> {
        self.store
            .events_since(thread_id, 0)
            .unwrap()
            .into_iter()
            .map(|event| event.kind)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn handoff_with(messages: &[(&str, &str)], native_session: &Value) -> Value {
    json!({
        "protocol_version": 1,
        "kind": "collect_handoff",
        "native_session": native_session,
        "transcript": {
            "messages": messages
                .iter()
                .map(|(role, text)| json!({"role": role, "text": text}))
                .collect::<Vec<_>>(),
        },
        "usage": {"input_tokens": 10, "plan_five_hour_percent": 42.0},
    })
}

#[test]
fn send_records_user_message_and_adapter_events_in_order() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_session = Some(json!("sess-1"));
    adapter.start_events = vec![
        json!({"event": "tool_call", "content": "Edit", "payload": {"name": "Edit"}}),
        json!({"event": "assistant_message", "role": "assistant", "content": "done"}),
        json!({"event": "usage", "usage": {"input_tokens": 5, "plan_week_percent": 7.5}}),
        json!({"event": "turn_complete"}),
    ];

    let turn_id = fixture
        .service()
        .send(&thread_id, "fix it".into(), &adapter)
        .unwrap();

    assert_eq!(
        fixture.kinds(&thread_id),
        vec![
            EventKind::ThreadCreated,
            EventKind::UserMessage,
            EventKind::ToolCall,
            EventKind::AssistantMessage,
            EventKind::Usage,
            EventKind::Status,
        ]
    );
    let events = fixture.store.events_since(&thread_id, 0).unwrap();
    assert!(
        events[1..]
            .iter()
            .all(|event| event.turn_id.as_deref() == Some(turn_id.as_str()))
    );
    assert_eq!(events[2].payload, Some(json!({"name": "Edit"})));
    assert_eq!(events[2].harness.as_deref(), Some("claude"));
    assert_eq!(
        fixture
            .store
            .plan_usage("claude")
            .unwrap()
            .unwrap()
            .plan_week_percent,
        Some(7.5)
    );
}

#[test]
fn first_send_starts_a_thread_and_later_sends_resume_it() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_session = Some(json!({"session": "abc"}));

    fixture
        .service()
        .send(&thread_id, "one".into(), &adapter)
        .unwrap();
    let bound = fixture.store.get_thread(&thread_id).unwrap();
    assert_eq!(bound.native_session, Some(json!({"session": "abc"})));

    fixture
        .service()
        .send(&thread_id, "two".into(), &adapter)
        .unwrap();

    assert_eq!(
        adapter.calls(),
        vec!["negotiate", "start_thread", "negotiate", "send_message"]
    );
    assert_eq!(
        adapter.sent.borrow().as_slice(),
        &[(Some(json!({"session": "abc"})), "two".to_owned())]
    );
}

#[test]
fn start_turn_hands_the_prior_conversation_to_the_new_session() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let adapter = FakeAdapter::new();

    fixture
        .service()
        .send(&thread_id, "remember the zebra".into(), &adapter)
        .unwrap();

    let contexts = adapter.started_with.borrow();
    assert_eq!(contexts.len(), 1);
    assert!(contexts[0].contains("remember the zebra"));
}

#[test]
fn send_without_a_harness_is_rejected_before_anything_is_recorded() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(None);
    let adapter = FakeAdapter::new();

    let error = fixture
        .service()
        .send(&thread_id, "hi".into(), &adapter)
        .unwrap_err();

    assert!(error.to_string().contains("no harness assigned"));
    assert_eq!(fixture.kinds(&thread_id), vec![EventKind::ThreadCreated]);
    assert!(adapter.calls().is_empty());
}

#[test]
fn unknown_adapter_event_aborts_the_turn_without_binding_a_session() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_session = Some(json!("never-bound"));
    adapter.start_events = vec![
        json!({"event": "assistant_message", "role": "assistant", "content": "partial"}),
        json!({"event": "bogus"}),
    ];

    let error = fixture
        .service()
        .send(&thread_id, "go".into(), &adapter)
        .unwrap_err();

    assert!(error.to_string().contains("unknown event kind"));
    assert_eq!(
        fixture.kinds(&thread_id),
        vec![
            EventKind::ThreadCreated,
            EventKind::UserMessage,
            EventKind::AssistantMessage,
        ]
    );
    assert_eq!(
        fixture.store.get_thread(&thread_id).unwrap().native_session,
        None
    );
}

#[test]
fn plan_mode_is_refused_when_the_harness_cannot_enforce_it() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("hermes"));
    fixture
        .store
        .set_thread_mode(&thread_id, ThreadMode::Plan)
        .unwrap();
    let mut adapter = FakeAdapter::new();
    adapter.capabilities = json!({"headless": true});

    let error = fixture
        .service()
        .send(&thread_id, "plan it".into(), &adapter)
        .unwrap_err();

    assert!(error.to_string().contains("cannot enforce plan mode"));
    assert!(!adapter.calls().contains(&"start_thread"));
}

#[test]
fn a_harness_without_headless_mode_is_refused() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.capabilities = json!({"headless": false});

    let error = fixture
        .service()
        .reassign_harness(&thread_id, "claude", &adapter)
        .unwrap_err();

    assert!(error.to_string().contains("no headless mode"));
}

#[test]
fn reassign_collects_evidence_records_a_carryover_and_rebinds() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    fixture
        .store
        .set_thread_harness(&thread_id, Some("claude"), Some(&json!("old")))
        .unwrap();
    let mut adapter = FakeAdapter::new();
    adapter.handoff = Some(handoff_with(
        &[("user", "what is 2+2"), ("assistant", "4")],
        &json!("old-refreshed"),
    ));

    fixture
        .service()
        .reassign_harness(&thread_id, "codex", &adapter)
        .unwrap();

    let thread = fixture.store.get_thread(&thread_id).unwrap();
    assert_eq!(thread.active_harness.as_deref(), Some("codex"));
    assert_eq!(thread.native_session, None);
    assert_eq!(
        fixture.kinds(&thread_id),
        vec![
            EventKind::ThreadCreated,
            EventKind::UserMessage,
            EventKind::AssistantMessage,
            EventKind::HandoffCarryover,
            EventKind::HarnessUnbound,
            EventKind::HarnessBound,
        ]
    );
    assert_eq!(
        fixture
            .store
            .plan_usage("claude")
            .unwrap()
            .unwrap()
            .plan_five_hour_percent,
        Some(42.0)
    );
}

#[test]
fn reassign_does_not_duplicate_messages_the_log_already_has() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    fixture
        .store
        .set_thread_harness(&thread_id, Some("claude"), Some(&json!("s")))
        .unwrap();
    let mut adapter = FakeAdapter::new();
    adapter.handoff = Some(handoff_with(
        &[("user", "what is 2+2"), ("assistant", " 4 ")],
        &json!("s"),
    ));
    fixture
        .store
        .append_event(
            &thread_id,
            crate::domain::NewThreadEvent {
                role: Some("assistant".into()),
                content: Some("4".into()),
                ..crate::domain::NewThreadEvent::new(EventKind::AssistantMessage)
            },
        )
        .unwrap();

    fixture
        .service()
        .reassign_harness(&thread_id, "codex", &adapter)
        .unwrap();

    let assistant_messages = fixture
        .kinds(&thread_id)
        .into_iter()
        .filter(|kind| *kind == EventKind::AssistantMessage)
        .count();
    assert_eq!(assistant_messages, 1);
    let user_messages = fixture
        .kinds(&thread_id)
        .into_iter()
        .filter(|kind| *kind == EventKind::UserMessage)
        .count();
    assert_eq!(user_messages, 1);
}

#[test]
fn reassign_leaves_the_thread_untouched_when_collection_fails() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    fixture
        .store
        .set_thread_harness(&thread_id, Some("claude"), Some(&json!("s")))
        .unwrap();
    let adapter = FakeAdapter::new();
    let before = fixture.store.events_since(&thread_id, 0).unwrap().len();

    let error = fixture
        .service()
        .reassign_harness(&thread_id, "codex", &adapter)
        .unwrap_err();

    assert!(error.to_string().contains("collection failed"));
    let thread = fixture.store.get_thread(&thread_id).unwrap();
    assert_eq!(thread.active_harness.as_deref(), Some("claude"));
    assert_eq!(thread.native_session, Some(json!("s")));
    assert_eq!(
        fixture.store.events_since(&thread_id, 0).unwrap().len(),
        before
    );
}

#[test]
fn reassign_skips_collection_when_there_is_no_native_session() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let adapter = FakeAdapter::new();

    fixture
        .service()
        .reassign_harness(&thread_id, "codex", &adapter)
        .unwrap();

    assert!(!adapter.calls().contains(&"collect_handoff"));
    assert_eq!(
        fixture
            .store
            .get_thread(&thread_id)
            .unwrap()
            .active_harness
            .as_deref(),
        Some("codex")
    );
}

#[test]
fn reassign_cancels_requests_still_pending() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events = vec![json!({
        "event": "request",
        "payload": {"id": "r1", "kind": "permission", "title": "Bash ls"},
    })];
    fixture
        .service()
        .send(&thread_id, "run ls".into(), &adapter)
        .unwrap();
    assert_eq!(
        fixture
            .service()
            .pending_requests(&thread_id)
            .unwrap()
            .len(),
        1
    );

    fixture
        .service()
        .reassign_harness(&thread_id, "codex", &adapter)
        .unwrap();

    assert!(
        fixture
            .service()
            .pending_requests(&thread_id)
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .kinds(&thread_id)
            .contains(&EventKind::RequestAnswered)
    );
}

#[test]
fn request_events_open_a_pending_request_instead_of_a_thread_event() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events = vec![json!({
        "event": "request",
        "payload": {"id": "native-1", "kind": "plan_approval", "title": "plan"},
    })];

    fixture
        .service()
        .send(&thread_id, "plan".into(), &adapter)
        .unwrap();

    let pending = fixture.service().pending_requests(&thread_id).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].status, RequestStatus::Pending);
    assert_eq!(pending[0].harness.as_deref(), Some("claude"));
    assert_eq!(pending[0].payload.as_ref().unwrap()["id"], "native-1");
}

#[test]
fn malformed_request_events_are_rejected() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    for payload in [
        json!(null),
        json!({"kind": "permission"}),
        json!({"id": "x"}),
        json!({"id": "x", "kind": "nonsense"}),
    ] {
        let mut adapter = FakeAdapter::new();
        adapter.start_events = vec![json!({"event": "request", "payload": payload})];
        assert!(
            fixture
                .service()
                .send(&thread_id, "go".into(), &adapter)
                .is_err()
        );
    }
    assert!(
        fixture
            .service()
            .pending_requests(&thread_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn answer_rejects_a_request_from_another_thread() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events = vec![json!({
        "event": "request",
        "payload": {"id": "r1", "kind": "permission", "title": "t"},
    })];
    fixture
        .service()
        .send(&thread_id, "go".into(), &adapter)
        .unwrap();
    let request = fixture
        .service()
        .pending_requests(&thread_id)
        .unwrap()
        .remove(0);

    let error = fixture
        .service()
        .answer("some-other-thread", &request.id, &json!({"allow": true}))
        .unwrap_err();

    assert!(error.to_string().contains("does not belong"));
    assert_eq!(
        fixture
            .service()
            .pending_requests(&thread_id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn answer_cancels_the_request_when_the_turn_is_gone() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events = vec![json!({
        "event": "request",
        "payload": {"id": "r1", "kind": "permission", "title": "t"},
    })];
    fixture
        .service()
        .send(&thread_id, "go".into(), &adapter)
        .unwrap();
    let request = fixture
        .service()
        .pending_requests(&thread_id)
        .unwrap()
        .remove(0);

    let error = fixture
        .service()
        .answer(&thread_id, &request.id, &json!({"allow": true}))
        .unwrap_err();

    assert!(format!("{error:#}").contains("no longer running"));
    assert!(
        fixture
            .service()
            .pending_requests(&thread_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture.store.get_request(&request.id).unwrap().status,
        RequestStatus::Cancelled
    );
}

#[test]
fn answering_an_already_resolved_request_fails() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events = vec![json!({
        "event": "request",
        "payload": {"id": "r1", "kind": "permission", "title": "t"},
    })];
    fixture
        .service()
        .send(&thread_id, "go".into(), &adapter)
        .unwrap();
    let request = fixture
        .service()
        .pending_requests(&thread_id)
        .unwrap()
        .remove(0);
    fixture.store.cancel_pending_requests(&thread_id).unwrap();

    let error = fixture
        .service()
        .answer(&thread_id, &request.id, &json!({"allow": true}))
        .unwrap_err();

    assert!(error.to_string().contains("is not pending"));
}

#[test]
fn archive_detaches_unbinds_and_marks_the_thread_archived() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let adapter = FakeAdapter::new();

    fixture.service().archive(&thread_id, &adapter).unwrap();

    assert_eq!(adapter.calls(), vec!["detach_thread"]);
    assert_eq!(
        fixture.store.get_thread(&thread_id).unwrap().status,
        ThreadStatus::Archived
    );
    assert!(
        fixture
            .kinds(&thread_id)
            .contains(&EventKind::HarnessUnbound)
    );
}

#[test]
fn delete_detaches_and_removes_the_thread() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let adapter = FakeAdapter::new();

    fixture.service().delete(&thread_id, &adapter).unwrap();

    assert_eq!(adapter.calls(), vec!["detach_thread"]);
    assert!(fixture.store.get_thread(&thread_id).is_err());
}

#[test]
fn set_mode_to_the_current_mode_records_nothing() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(None);
    let before = fixture.kinds(&thread_id);

    fixture
        .service()
        .set_mode(&thread_id, ThreadMode::Build)
        .unwrap();
    assert_eq!(fixture.kinds(&thread_id), before);

    fixture
        .service()
        .set_mode(&thread_id, ThreadMode::Plan)
        .unwrap();
    assert_eq!(
        fixture.kinds(&thread_id).last(),
        Some(&EventKind::ModeChanged)
    );
}

#[test]
fn attach_is_a_no_op_for_a_harness_that_cannot_run_headless() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.capabilities = json!({"headless": false});

    fixture.service().attach(&thread_id, &adapter).unwrap();

    assert_eq!(adapter.calls(), vec!["negotiate"]);
}

#[test]
fn attach_without_a_harness_does_not_call_the_adapter() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(None);
    let adapter = FakeAdapter::new();

    fixture.service().attach(&thread_id, &adapter).unwrap();

    assert!(adapter.calls().is_empty());
}

#[test]
fn sync_returns_only_events_after_the_cursor() {
    let fixture = Fixture::new();
    let thread_id = fixture.thread(Some("claude"));
    let mut adapter = FakeAdapter::new();
    adapter.start_events =
        vec![json!({"event": "assistant_message", "role": "assistant", "content": "a"})];
    fixture
        .service()
        .send(&thread_id, "q".into(), &adapter)
        .unwrap();

    let events = fixture.service().sync(&thread_id, 2).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, EventKind::AssistantMessage);
    assert!(fixture.service().sync(&thread_id, 3).unwrap().is_empty());
}
