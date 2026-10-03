use anyhow::{Context, Result};
use libsql::{Row, params};
use serde_json::Value;

use super::Store;
use super::queries::{decode_json, encode_json};
use crate::domain::{EventKind, NewThreadEvent, Thread, ThreadMode, ThreadStatus, id, now};

const COLUMNS: &str = "id, workspace_id, status, mode, active_harness, native_session, last_event_seq, created_at, updated_at";

fn thread_from_row(row: &Row) -> Result<Thread> {
    Ok(Thread {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        status: row.get::<String>(2)?.parse()?,
        mode: row.get::<String>(3)?.parse()?,
        active_harness: row.get(4)?,
        native_session: decode_json(row.get(5)?)?,
        last_event_seq: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl Store {
    pub fn create_thread(&self, workspace_id: &str) -> Result<Thread> {
        let created_at = now();
        let thread = Thread {
            id: id(),
            workspace_id: workspace_id.to_string(),
            status: ThreadStatus::Active,
            mode: ThreadMode::default(),
            active_harness: None,
            native_session: None,
            last_event_seq: 0,
            created_at: created_at.clone(),
            updated_at: created_at.clone(),
        };
        self.execute(
            &format!("INSERT INTO threads ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"),
            params![
                thread.id.clone(),
                thread.workspace_id.clone(),
                thread.status.as_str(),
                thread.mode.as_str(),
                thread.active_harness.clone(),
                Option::<String>::None,
                thread.last_event_seq,
                thread.created_at.clone(),
                thread.updated_at.clone()
            ],
        )?;
        self.append_event_at(
            &thread.id,
            NewThreadEvent::new(EventKind::ThreadCreated),
            created_at,
        )?;
        self.get_thread(&thread.id)
    }

    pub fn get_thread(&self, id: &str) -> Result<Thread> {
        self.query_optional(
            &format!("SELECT {COLUMNS} FROM threads WHERE id = ?1"),
            params![id],
            thread_from_row,
        )?
        .context("no such thread")
    }

    pub fn list_threads(&self, workspace_id: &str) -> Result<Vec<Thread>> {
        self.query_all(
            &format!(
                "SELECT {COLUMNS} FROM threads WHERE workspace_id = ?1 ORDER BY updated_at DESC"
            ),
            params![workspace_id],
            thread_from_row,
        )
    }

    pub fn set_thread_harness(
        &self,
        thread_id: &str,
        harness: Option<&str>,
        native_session: Option<&Value>,
    ) -> Result<()> {
        self.execute(
            "UPDATE threads SET active_harness = ?1, native_session = ?2, updated_at = ?3 WHERE id = ?4",
            params![harness, encode_json(native_session)?, now(), thread_id],
        )?;
        Ok(())
    }

    pub fn set_thread_status(&self, thread_id: &str, status: ThreadStatus) -> Result<()> {
        self.execute(
            "UPDATE threads SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now(), thread_id],
        )?;
        Ok(())
    }

    pub fn delete_thread(&self, thread_id: &str) -> Result<()> {
        self.block_on(async {
            let tx = self.conn.transaction().await?;
            for table in [
                "thread_events",
                "harness_bindings",
                "harness_requests",
                "usage_snapshots",
                "hook_observations",
                "handoff_carryovers",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE thread_id = ?1"),
                    params![thread_id],
                )
                .await?;
            }
            tx.execute("DELETE FROM threads WHERE id = ?1", params![thread_id])
                .await?;
            tx.commit().await?;
            Ok(())
        })
    }

    pub fn set_thread_mode(&self, thread_id: &str, mode: ThreadMode) -> Result<()> {
        self.execute(
            "UPDATE threads SET mode = ?1, updated_at = ?2 WHERE id = ?3",
            params![mode.as_str(), now(), thread_id],
        )?;
        self.append_event(
            thread_id,
            NewThreadEvent {
                payload: Some(serde_json::json!({ "mode": mode.as_str() })),
                ..NewThreadEvent::new(EventKind::ModeChanged)
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::domain::{EventKind, ThreadMode};
    use crate::store::fixtures::{test_store, test_thread};

    #[test]
    fn create_thread_records_a_thread_created_event() {
        let (store, root) = test_store();
        let workspace = store
            .workspace_for(Path::new("/workspace"), None, "key-b")
            .unwrap();
        let thread = store.create_thread(&workspace.id).unwrap();

        assert_eq!(thread.last_event_seq, 1);
        let events = store.events_since(&thread.id, 0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ThreadCreated);
        assert_eq!(events[0].seq, 1);
        assert_eq!(thread.created_at, thread.updated_at);
        assert_eq!(events[0].created_at, thread.created_at);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_thread_removes_the_thread_and_its_rows() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-e");
        store.open_binding(&thread.id, "claude", None).unwrap();

        store.delete_thread(&thread.id).unwrap();

        assert!(store.get_thread(&thread.id).is_err());
        assert_eq!(store.events_since(&thread.id, 0).unwrap().len(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thread_mode_defaults_to_build_and_persists_with_an_event() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-f");
        assert_eq!(thread.mode, ThreadMode::Build);

        store.set_thread_mode(&thread.id, ThreadMode::Plan).unwrap();

        assert_eq!(store.get_thread(&thread.id).unwrap().mode, ThreadMode::Plan);
        let events = store.events_since(&thread.id, 1).unwrap();
        assert_eq!(events.last().unwrap().kind, EventKind::ModeChanged);
        fs::remove_dir_all(root).unwrap();
    }
}
