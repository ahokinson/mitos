use std::path::{Path, PathBuf};

use anyhow::Result;
use libsql::params;

use super::Store;
use crate::domain::{HarnessKind, Thread, id};

/// Account-wide, not thread-scoped.
pub struct HarnessPlanUsage {
    pub plan_five_hour_percent: Option<f64>,
    pub plan_week_percent: Option<f64>,
}

impl Store {
    pub fn plan_usage(&self, harness: HarnessKind) -> Result<Option<HarnessPlanUsage>> {
        self.query_optional(
            "SELECT plan_five_hour_percent, plan_week_percent FROM harness_plan_usage WHERE harness = ?1",
            params![harness.as_str()],
            |row| {
                Ok(HarnessPlanUsage {
                    plan_five_hour_percent: row.get(0)?,
                    plan_week_percent: row.get(1)?,
                })
            },
        )
    }
}

pub(super) fn test_store() -> (Store, PathBuf) {
    let root = std::env::temp_dir().join(format!("mitos-store-test-{}", id()));
    (Store::new(Some(root.clone())).unwrap(), root)
}

pub(super) fn test_thread(store: &Store, workspace_key: &str) -> Thread {
    let workspace = store
        .workspace_for(Path::new("/workspace"), None, workspace_key)
        .unwrap();
    store.create_thread(&workspace.id).unwrap()
}
