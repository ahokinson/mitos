use std::path::Path;

use anyhow::{Context, Result};
use libsql::params::IntoParams;
use libsql::{Connection, Database, OpenFlags, Value as SqlValue};
use serde_json::Value;
use tokio::runtime::{Builder, Runtime};

use crate::json::Record;

/// A harness's own SQLite history, opened read-only; rows come back as JSON
/// records keyed by column name, so a column a newer or older harness lacks
/// simply reads as absent.
pub struct ReadonlyDatabase {
    rt: Runtime,
    conn: Connection,
    _db: Database,
}

impl ReadonlyDatabase {
    /// `None` when the file is missing or cannot be opened.
    pub fn open(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let rt = Builder::new_current_thread().build().ok()?;
        let (db, conn) = rt.block_on(async {
            let db = libsql::Builder::new_local(path)
                .flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
                .build()
                .await
                .ok()?;
            let conn = db.connect().ok()?;
            Some((db, conn))
        })?;
        Some(Self { rt, conn, _db: db })
    }

    pub fn query(&self, sql: &str, params: impl IntoParams) -> Result<Vec<Record>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(sql, params).await?;
            let mut records = Vec::new();
            while let Some(row) = rows.next().await? {
                let mut record = Record::new();
                for index in 0..row.column_count() {
                    let name = row
                        .column_name(index)
                        .with_context(|| format!("column {index} has no name"))?;
                    record.insert(name.to_string(), json_value(row.get_value(index)?));
                }
                records.push(record);
            }
            Ok(records)
        })
    }
}

fn json_value(value: SqlValue) -> Value {
    match value {
        SqlValue::Integer(number) => Value::from(number),
        SqlValue::Real(number) => {
            serde_json::Number::from_f64(number).map_or(Value::Null, Value::Number)
        }
        SqlValue::Text(text) => Value::String(text),
        SqlValue::Null | SqlValue::Blob(_) => Value::Null,
    }
}

/// Creates a database file by running `statements`, for tests that need a
/// harness's history to read.
#[cfg(test)]
pub fn create_database(path: &Path, statements: &str) {
    let rt = Builder::new_current_thread().build().unwrap();
    rt.block_on(async {
        let db = libsql::Builder::new_local(path).build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute_batch(statements).await.unwrap();
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rows_become_records_keyed_by_column_and_a_missing_file_is_none() {
        let dir = std::env::temp_dir().join(format!("mitos-db-{}", crate::domain::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.db");
        create_database(
            &path,
            "CREATE TABLE t (id INTEGER, name TEXT, score REAL, note TEXT); \
             INSERT INTO t VALUES (1, 'a', 1.5, NULL);",
        );
        let database = ReadonlyDatabase::open(&path).unwrap();
        let rows = database.query("SELECT * FROM t WHERE id = ?", [1]).unwrap();
        assert_eq!(
            Value::Object(rows[0].clone()),
            json!({ "id": 1, "name": "a", "score": 1.5, "note": null })
        );
        assert!(database.query("SELECT * FROM missing", ()).is_err());
        assert!(ReadonlyDatabase::open(&dir.join("absent.db")).is_none());
        drop(database);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
