use anyhow::{Result, bail};
use serde_json::Value;

use super::ThreadService;
use crate::domain::{EventKind, HarnessKind, NewThreadEvent, ThreadId, ThreadMode, UnbindReason};
use crate::wire::responses::Capabilities;

impl ThreadService<'_> {
    pub(super) fn bind_harness(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
        native_session: Option<&Value>,
    ) -> Result<()> {
        self.store
            .set_thread_harness(thread_id, Some(harness), native_session)?;
        self.store
            .open_binding(thread_id, harness, native_session)?;
        Ok(())
    }

    pub(super) fn unbind_harness(
        &self,
        thread_id: &ThreadId,
        harness: HarnessKind,
        reason: UnbindReason,
    ) -> Result<()> {
        self.store.close_open_binding(thread_id, harness, reason)?;
        self.store.append_event(
            thread_id,
            NewThreadEvent {
                harness: Some(harness),
                ..NewThreadEvent::new(EventKind::HarnessUnbound)
            },
        )?;
        Ok(())
    }
}

pub(super) fn ensure_supported(
    harness: HarnessKind,
    capabilities: &Capabilities,
    mode: ThreadMode,
) -> Result<()> {
    if !capabilities.headless {
        bail!("{harness} has no headless mode; use `mitos enter {harness}` instead");
    }
    if mode != ThreadMode::Build && !capabilities.modes.contains(&mode) {
        bail!("{harness} cannot enforce {} mode", mode.as_str());
    }
    Ok(())
}
