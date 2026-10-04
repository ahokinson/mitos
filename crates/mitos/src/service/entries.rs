use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;

use super::ThreadService;
use super::notes::ThreadNotes;
use crate::domain::UnbindReason;
use crate::git::Checkout;
use crate::ports::HarnessAdapter;
use crate::store::StoreLock;
use crate::wire::requests::AdapterRequest;
use crate::wire::responses::LaunchPlan;

pub struct Entry {
    pub launch: LaunchPlan,
    pub context: String,
    pub workdir: PathBuf,
    thread_id: String,
    harness: String,
    workspace_root: String,
    _lock: StoreLock,
}

impl ThreadService<'_> {
    /// The returned entry holds the thread lock until `finish_entry`.
    pub fn begin_entry<A: HarnessAdapter>(
        &self,
        thread_id: &str,
        harness: &str,
        native_session: Option<String>,
        adapter: &A,
    ) -> Result<Entry> {
        let thread = self.store.get_thread(thread_id)?;
        let lock = self.store.lock_thread(&thread.id)?;
        let workspace = self.store.get_workspace(&thread.workspace_id)?;
        let checkout = Checkout::open(&workspace.root)?;
        let from_harness = thread.active_harness.clone();

        if from_harness.as_deref() != Some(harness) {
            self.collect_handoff_evidence(&thread, &workspace.root, adapter)?;
            if let Some(from) = from_harness {
                self.unbind_harness(&thread.id, &from, UnbindReason::Reassigned)?;
            }
        }

        if let Some(native_session) = native_session {
            self.store.set_thread_harness(
                &thread.id,
                Some(harness),
                Some(&Value::String(native_session)),
            )?;
        }
        let thread = self.store.get_thread(&thread.id)?;

        let context = self.render_launch_handoff(&thread)?;
        let target_native_session = (thread.active_harness.as_deref() == Some(harness))
            .then_some(thread.native_session.clone())
            .flatten();
        let request = AdapterRequest::prepare_launch(
            harness,
            checkout.root(),
            context.clone(),
            target_native_session,
        );
        let mut launch = adapter.prepare_launch(&request)?;
        // Hooks the harness fires inherit this, so `mitos hook` knows the thread.
        launch
            .env
            .insert("MITOS_THREAD_ID".into(), thread.id.clone());
        self.bind_harness(&thread.id, harness, launch.native_session.as_ref())?;
        Ok(Entry {
            launch,
            context,
            workdir: checkout.root().to_path_buf(),
            thread_id: thread.id,
            harness: harness.into(),
            workspace_root: workspace.root,
            _lock: lock,
        })
    }

    /// A mining failure is returned, not raised, so the caller can carry on.
    pub fn finish_entry<A: HarnessAdapter>(
        &self,
        entry: Entry,
        notes: ThreadNotes,
        adapter: &A,
    ) -> Result<Option<anyhow::Error>> {
        let Entry {
            thread_id,
            harness,
            workspace_root,
            _lock: lock,
            ..
        } = entry;
        let current = self.store.get_thread(&thread_id)?;
        let mining_failure = self
            .collect_handoff_evidence(&current, &workspace_root, adapter)
            .err();
        self.record_notes(&thread_id, Some(&harness), notes)?;
        drop(lock);
        Ok(mining_failure)
    }
}
