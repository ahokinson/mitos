mod bindings;
mod carryovers;
mod events;
#[cfg(test)]
mod fixtures;
mod hooks;
mod locks;
mod queries;
mod requests;
mod schemas;
mod threads;
mod usage;
mod views;
mod workspaces;

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use libsql::{Connection, Database};

use crate::config::resolve_root;

pub use carryovers::NewHandoffCarryover;
pub use locks::StoreLock;
pub use requests::NewHarnessRequest;
pub use usage::{PlanUsageFields, UsageFields};

pub struct Store {
    root: PathBuf,
    rt: tokio::runtime::Runtime,
    conn: Connection,
    _db: Database,
}

impl Store {
    pub fn new(override_dir: Option<PathBuf>) -> Result<Self> {
        let root = resolve_root(override_dir);
        fs::create_dir_all(&root)?;
        fs::create_dir_all(root.join("locks"))?;
        let db_path = root.join("mitos.db");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .context("could not start the async runtime backing Mitos's database")?;
        let (db, conn) = rt.block_on(async {
            let db = libsql::Builder::new_local(&db_path)
                .build()
                .await
                .with_context(|| format!("could not open {}", db_path.display()))?;
            let conn = db.connect()?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;",
            )
            .await?;
            schemas::migrate(&conn).await?;
            anyhow::Ok((db, conn))
        })?;
        Ok(Self {
            root,
            rt,
            conn,
            _db: db,
        })
    }

    fn block_on<F: Future>(&self, fut: F) -> F::Output {
        self.rt.block_on(fut)
    }

    pub fn config_root(&self) -> &Path {
        &self.root
    }

    pub fn lock_thread(&self, id: &str) -> Result<StoreLock> {
        locks::lock_file(&self.root.join("locks").join(format!("thread-{id}.lock")))
    }
}
