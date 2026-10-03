use anyhow::{Result, bail};
use libsql::Connection;

const SCHEMA_VERSION: i64 = 1;
const MIGRATION_0001: &str = include_str!("../../migrations/0001_init.sql");

pub(super) async fn migrate(conn: &Connection) -> Result<()> {
    let mut rows = conn.query("PRAGMA user_version", ()).await?;
    let version: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    drop(rows);
    if version > SCHEMA_VERSION {
        bail!(
            "mitos.db schema version {version} is newer than this binary supports ({SCHEMA_VERSION}); update Mitos"
        );
    }
    if version < 1 {
        conn.execute_batch(MIGRATION_0001).await?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
            .await?;
    }
    Ok(())
}
