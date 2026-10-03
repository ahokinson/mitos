use anyhow::{Context, Result};
use libsql::{Row, params};

use super::Store;
use super::queries::{decode_json, encode_json};
use crate::domain::{NewThreadEvent, ThreadEvent, now};

const COLUMNS: &str =
    "thread_id, seq, turn_id, harness, kind, role, content, payload_json, created_at";

fn thread_event_from_row(row: &Row) -> Result<ThreadEvent> {
    Ok(ThreadEvent {
        thread_id: row.get(0)?,
        seq: row.get(1)?,
        turn_id: row.get(2)?,
        harness: row.get(3)?,
        kind: row.get::<String>(4)?.parse()?,
        role: row.get(5)?,
        content: row.get(6)?,
        payload: decode_json(row.get(7)?)?,
        created_at: row.get(8)?,
    })
}

impl Store {
    pub fn append_event(&self, thread_id: &str, event: NewThreadEvent) -> Result<ThreadEvent> {
        self.append_event_at(thread_id, event, now())
    }

    pub(super) fn append_event_at(
        &self,
        thread_id: &str,
        event: NewThreadEvent,
        created_at: String,
    ) -> Result<ThreadEvent> {
        let kind = event.kind.context("event kind is required")?;
        let payload_json = encode_json(event.payload.as_ref())?;
        self.block_on(async {
            let tx = self.conn.transaction().await?;
            let mut rows = tx
                .query(
                    "SELECT last_event_seq FROM threads WHERE id = ?1",
                    params![thread_id],
                )
                .await?;
            let row = rows.next().await?.context("no such thread")?;
            let last_seq: i64 = row.get(0)?;
            drop(rows);
            let seq = last_seq + 1;
            tx.execute(
                &format!(
                    "INSERT INTO thread_events ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"
                ),
                params![
                    thread_id,
                    seq,
                    event.turn_id.clone(),
                    event.harness.clone(),
                    kind.as_str(),
                    event.role.clone(),
                    event.content.clone(),
                    payload_json,
                    created_at.clone()
                ],
            )
            .await?;
            tx.execute(
                "UPDATE threads SET last_event_seq = ?1, updated_at = ?2 WHERE id = ?3",
                params![seq, created_at.clone(), thread_id],
            )
            .await?;
            tx.commit().await?;
            Ok(ThreadEvent {
                thread_id: thread_id.to_string(),
                seq,
                turn_id: event.turn_id,
                harness: event.harness,
                kind,
                role: event.role,
                content: event.content,
                payload: event.payload,
                created_at,
            })
        })
    }

    pub fn user_message_for_turn(&self, thread_id: &str, turn_id: &str) -> Result<String> {
        let content = self
            .query_optional(
                "SELECT content FROM thread_events \
                 WHERE thread_id = ?1 AND turn_id = ?2 AND kind = 'user_message' \
                 ORDER BY seq DESC LIMIT 1",
                params![thread_id, turn_id],
                |row| Ok(row.get::<Option<String>>(0)?),
            )?
            .context("no user_message event for this turn")?;
        content.context("user_message event has no content")
    }

    pub fn events_since(&self, thread_id: &str, since_seq: i64) -> Result<Vec<ThreadEvent>> {
        self.query_all(
            &format!(
                "SELECT {COLUMNS} FROM thread_events WHERE thread_id = ?1 AND seq > ?2 ORDER BY seq ASC"
            ),
            params![thread_id, since_seq],
            thread_event_from_row,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::domain::{EventKind, NewThreadEvent};
    use crate::store::fixtures::{test_store, test_thread};

    #[test]
    fn append_event_assigns_increasing_sequence_numbers() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-c");

        let event = store
            .append_event(
                &thread.id,
                NewThreadEvent {
                    content: Some("hello".into()),
                    ..NewThreadEvent::new(EventKind::UserMessage)
                },
            )
            .unwrap();
        assert_eq!(event.seq, 2);
        assert_eq!(store.get_thread(&thread.id).unwrap().last_event_seq, 2);

        let since_first = store.events_since(&thread.id, 1).unwrap();
        assert_eq!(since_first.len(), 1);
        assert_eq!(since_first[0].content.as_deref(), Some("hello"));
        fs::remove_dir_all(root).unwrap();
    }
}
