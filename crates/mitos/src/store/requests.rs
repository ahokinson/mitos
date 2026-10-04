use anyhow::Result;
use libsql::{Row, params};
use serde_json::Value;

use super::queries::{decode_json, encode_json, optional_text, parse_optional, text};
use super::{Store, StoreError};
use crate::domain::{
    EventKind, HarnessKind, HarnessRequest, NewThreadEvent, RequestId, RequestKind, RequestStatus,
    ThreadId, TurnId, now,
};

const COLUMNS: &str = "id, thread_id, turn_id, harness, kind, payload_json, status, response_json, created_at, answered_at";

fn request_from_row(row: &Row) -> Result<HarnessRequest> {
    Ok(HarnessRequest {
        id: text(row, 0)?,
        thread_id: text(row, 1)?,
        turn_id: optional_text(row, 2)?,
        harness: parse_optional(row.get(3)?)?,
        kind: row.get::<String>(4)?.parse()?,
        payload: decode_json(row.get(5)?)?,
        status: row.get::<String>(6)?.parse()?,
        response: decode_json(row.get(7)?)?,
        created_at: text(row, 8)?,
        answered_at: optional_text(row, 9)?,
    })
}

#[derive(Clone, Debug)]
pub struct NewHarnessRequest {
    pub thread_id: ThreadId,
    pub turn_id: Option<TurnId>,
    pub harness: Option<HarnessKind>,
    pub kind: RequestKind,
    pub payload: Option<Value>,
}

impl Store {
    pub fn open_request(&self, request: NewHarnessRequest) -> Result<HarnessRequest> {
        let created = HarnessRequest {
            id: RequestId::generate(),
            thread_id: request.thread_id,
            turn_id: request.turn_id,
            harness: request.harness,
            kind: request.kind,
            payload: request.payload,
            status: RequestStatus::Pending,
            response: None,
            created_at: now(),
            answered_at: None,
        };
        self.execute(
            "INSERT INTO harness_requests \
             (id, thread_id, turn_id, harness, kind, payload_json, status, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                created.id.as_str(),
                created.thread_id.as_str(),
                created.turn_id.as_ref().map(TurnId::as_str),
                created.harness.map(HarnessKind::as_str),
                created.kind.as_str(),
                encode_json(created.payload.as_ref())?,
                created.status.as_str(),
                created.created_at.as_str()
            ],
        )?;
        self.append_event(
            &created.thread_id,
            NewThreadEvent {
                turn_id: created.turn_id.clone(),
                harness: created.harness,
                payload: Some(serde_json::json!({
                    "request_id": created.id,
                    "kind": created.kind.as_str(),
                    "request": created.payload,
                })),
                ..NewThreadEvent::new(EventKind::RequestOpened)
            },
        )?;
        Ok(created)
    }

    pub fn get_request(&self, request_id: &RequestId) -> Result<HarnessRequest> {
        self.query_optional(
            &format!("SELECT {COLUMNS} FROM harness_requests WHERE id = ?1"),
            params![request_id.as_str()],
            request_from_row,
        )?
        .ok_or(StoreError::NotFound("request").into())
    }

    pub fn pending_requests(&self, thread_id: &ThreadId) -> Result<Vec<HarnessRequest>> {
        self.query_all(
            &format!(
                "SELECT {COLUMNS} FROM harness_requests \
                 WHERE thread_id = ?1 AND status = 'pending' ORDER BY created_at ASC"
            ),
            params![thread_id.as_str()],
            request_from_row,
        )
    }

    pub fn answer_request(
        &self,
        request_id: &RequestId,
        response: &Value,
    ) -> Result<HarnessRequest> {
        self.resolve_request(request_id, RequestStatus::Answered, Some(response))
    }

    pub fn cancel_pending_requests(&self, thread_id: &ThreadId) -> Result<usize> {
        let pending = self.pending_requests(thread_id)?;
        for request in &pending {
            self.resolve_request(&request.id, RequestStatus::Cancelled, None)?;
        }
        Ok(pending.len())
    }

    fn resolve_request(
        &self,
        request_id: &RequestId,
        status: RequestStatus,
        response: Option<&Value>,
    ) -> Result<HarnessRequest> {
        let changed = self.execute(
            "UPDATE harness_requests SET status = ?1, response_json = ?2, answered_at = ?3 \
             WHERE id = ?4 AND status = 'pending'",
            params![
                status.as_str(),
                encode_json(response)?,
                now().as_str(),
                request_id.as_str()
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::RequestNotPending(request_id.clone()).into());
        }
        let resolved = self.get_request(request_id)?;
        self.append_event(
            &resolved.thread_id,
            NewThreadEvent {
                turn_id: resolved.turn_id.clone(),
                harness: resolved.harness,
                payload: Some(serde_json::json!({
                    "request_id": resolved.id,
                    "status": resolved.status.as_str(),
                    "response": resolved.response,
                })),
                ..NewThreadEvent::new(EventKind::RequestAnswered)
            },
        )?;
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::NewHarnessRequest;
    use crate::domain::{EventKind, HarnessKind, RequestKind, RequestStatus, TurnId};
    use crate::store::fixtures::{test_store, test_thread};

    #[test]
    fn request_lifecycle_opens_answers_once_and_records_events() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-g");
        let request = store
            .open_request(NewHarnessRequest {
                thread_id: thread.id.clone(),
                turn_id: Some(TurnId::from("turn-1")),
                harness: Some(HarnessKind::Claude),
                kind: RequestKind::Permission,
                payload: Some(serde_json::json!({ "tool": "Bash" })),
            })
            .unwrap();
        assert_eq!(store.pending_requests(&thread.id).unwrap().len(), 1);

        let answered = store
            .answer_request(&request.id, &serde_json::json!({ "allow": true }))
            .unwrap();
        assert_eq!(answered.status, RequestStatus::Answered);
        assert!(store.pending_requests(&thread.id).unwrap().is_empty());
        assert!(
            store
                .answer_request(&request.id, &serde_json::json!({ "allow": false }))
                .is_err()
        );

        let kinds: Vec<_> = store
            .events_since(&thread.id, 1)
            .unwrap()
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(
            kinds,
            [EventKind::RequestOpened, EventKind::RequestAnswered]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancel_pending_requests_resolves_every_open_request() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-h");
        for _ in 0..2 {
            store
                .open_request(NewHarnessRequest {
                    thread_id: thread.id.clone(),
                    turn_id: None,
                    harness: None,
                    kind: RequestKind::Question,
                    payload: None,
                })
                .unwrap();
        }
        assert_eq!(store.cancel_pending_requests(&thread.id).unwrap(), 2);
        assert!(store.pending_requests(&thread.id).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
