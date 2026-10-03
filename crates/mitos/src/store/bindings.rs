use anyhow::Result;
use libsql::params;
use serde_json::Value;

use super::Store;
use super::queries::encode_json;
use crate::domain::{HarnessBinding, UnbindReason, id, now};

impl Store {
    pub fn open_binding(
        &self,
        thread_id: &str,
        harness: &str,
        native_session: Option<&Value>,
    ) -> Result<HarnessBinding> {
        let binding = HarnessBinding {
            id: id(),
            thread_id: thread_id.to_string(),
            harness: harness.to_string(),
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
                binding.thread_id.clone(),
                binding.harness.clone(),
                encode_json(native_session)?,
                binding.bound_at.clone()
            ],
        )?;
        Ok(binding)
    }

    pub fn close_open_binding(
        &self,
        thread_id: &str,
        harness: &str,
        reason: UnbindReason,
    ) -> Result<()> {
        self.execute(
            "UPDATE harness_bindings SET unbound_at = ?1, unbind_reason = ?2 \
             WHERE thread_id = ?3 AND harness = ?4 AND unbound_at IS NULL",
            params![now(), reason.as_str(), thread_id, harness],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::domain::UnbindReason;
    use crate::store::fixtures::{test_store, test_thread};

    #[test]
    fn harness_binding_lifecycle_opens_and_closes() {
        let (store, root) = test_store();
        let thread = test_thread(&store, "key-d");
        store.open_binding(&thread.id, "claude", None).unwrap();
        store
            .close_open_binding(&thread.id, "claude", UnbindReason::Reassigned)
            .unwrap();
        store
            .close_open_binding(&thread.id, "claude", UnbindReason::Reassigned)
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
