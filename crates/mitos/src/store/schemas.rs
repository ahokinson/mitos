use anyhow::{Result, bail};
use libsql::Connection;

const CREATE_MIGRATION_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS schema_migrations (
        id TEXT PRIMARY KEY,
        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
";
const INITIAL_MIGRATION: &str = "0001_init";
const MIGRATION_0001: &str = include_str!("../../migrations/0001_init.sql");
const MIGRATIONS: [Migration; 1] = [Migration {
    id: INITIAL_MIGRATION,
    sql: MIGRATION_0001,
}];

struct Migration {
    id: &'static str,
    sql: &'static str,
}

pub(super) async fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(CREATE_MIGRATION_TABLE).await?;
    let mut applied = applied_migrations(conn).await?;
    if applied.is_empty() && has_initial_schema(conn).await? {
        conn.execute(
            "INSERT INTO schema_migrations (id) VALUES (?1)",
            [INITIAL_MIGRATION],
        )
        .await?;
        applied.push(INITIAL_MIGRATION.into());
    }
    for id in &applied {
        if !MIGRATIONS.iter().any(|migration| migration.id == id) {
            bail!("mitos.db records an unknown migration: {id}");
        }
    }
    for migration in MIGRATIONS {
        if applied.iter().any(|id| id == migration.id) {
            continue;
        }
        let sql = format!(
            "{}\nINSERT INTO schema_migrations (id) VALUES ('{}');",
            migration.sql, migration.id
        );
        conn.execute_transactional_batch(&sql).await?;
    }
    Ok(())
}

async fn applied_migrations(conn: &Connection) -> Result<Vec<String>> {
    let mut rows = conn
        .query("SELECT id FROM schema_migrations ORDER BY id", ())
        .await?;
    let mut migrations = Vec::new();
    while let Some(row) = rows.next().await? {
        migrations.push(row.get(0)?);
    }
    Ok(migrations)
}

async fn has_initial_schema(conn: &Connection) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            ["workspaces"],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use libsql::Database;

    use super::*;

    async fn test_database() -> (Database, Connection, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("mitos-schema-test-{}.db", crate::domain::id()));
        let database = libsql::Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        (database, connection, path)
    }

    fn remove_database(database: Database, connection: Connection, path: PathBuf) {
        drop(connection);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn records_and_skips_the_initial_migration() {
        let (database, connection, path) = test_database().await;
        migrate(&connection).await.unwrap();
        assert_eq!(
            applied_migrations(&connection).await.unwrap(),
            [INITIAL_MIGRATION]
        );
        migrate(&connection).await.unwrap();
        assert_eq!(
            applied_migrations(&connection).await.unwrap(),
            [INITIAL_MIGRATION]
        );
        remove_database(database, connection, path);
    }

    #[tokio::test]
    async fn records_the_initial_migration_for_an_existing_database() {
        let (database, connection, path) = test_database().await;
        connection.execute_batch(MIGRATION_0001).await.unwrap();
        migrate(&connection).await.unwrap();
        assert_eq!(
            applied_migrations(&connection).await.unwrap(),
            [INITIAL_MIGRATION]
        );
        remove_database(database, connection, path);
    }

    #[tokio::test]
    async fn rejects_an_unknown_recorded_migration() {
        let (database, connection, path) = test_database().await;
        connection
            .execute_batch(CREATE_MIGRATION_TABLE)
            .await
            .unwrap();
        connection
            .execute("INSERT INTO schema_migrations (id) VALUES ('unknown')", ())
            .await
            .unwrap();
        assert!(
            migrate(&connection)
                .await
                .unwrap_err()
                .to_string()
                .contains("unknown migration")
        );
        remove_database(database, connection, path);
    }
}
