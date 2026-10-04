use anyhow::Result;
use libsql::params;
use serde_json::Value;

use super::Store;
use super::queries::encode_json;
use crate::domain::{HarnessBinding, HarnessKind, ThreadId, UnbindReason, id, now};

impl Store {
    pub fn open_binding(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
        native_session: Option<&Value>,
    ) -> Result<HarnessBinding> {
        let binding = HarnessBinding {
            id: id(),
            thread_id: thread_id.clone(),
            harness,
            native_session: native_session.cloned(),
            bound_at: now(),
            unbound_at: None,
            unbind_reason: None,
        };
        self.execute(
            "INSERT INTO harness_bindings (id, thread_id, harness, native_session, bound_at) \
             VALUES (?1,?2,?3,?4,?5)",
            params![
                binding.id.clone(),
                binding.thread_id.as_str(),
                binding.harness.as_str(),
                encode_json(native_session)?,
                binding.bound_at.as_str()
            ],
        )?;
        Ok(binding)
    }

    pub fn close_open_binding(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
        reason: UnbindReason,
    ) -> Result<()> {
        self.execute(
            "UPDATE harness_bindings SET unbound_at = ?1, unbind_reason = ?2 \
             WHERE thread_id = ?3 AND harness = ?4 AND unbound_at IS NULL",
            params![
                now().as_str(),
                reason.as_str(),
                thread_id.as_str(),
                harness.as_str()
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::domain::{HarnessKind, UnbindReason};
    use crate::store::fixtures::{test_store, test_thread};

    #[test]
    fn harness_binding_lifecycle_opens_and_closes() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-d");
        store
            .open_binding(&thread.id, HarnessKind::Claude, None)
            .unwrap();
        store
            .close_open_binding(&thread.id, HarnessKind::Claude, UnbindReason::Reassigned)
            .unwrap();
        store
            .close_open_binding(&thread.id, HarnessKind::Claude, UnbindReason::Reassigned)
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
