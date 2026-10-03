use anyhow::Result;
use libsql::Row;
use libsql::params::IntoParams;
use serde_json::Value;

use super::Store;

pub(super) fn encode_json(value: Option<&Value>) -> Result<Option<String>> {
    Ok(value.map(serde_json::to_string).transpose()?)
}

pub(super) fn decode_json(text: Option<String>) -> Result<Option<Value>> {
    Ok(text.map(|text| serde_json::from_str(&text)).transpose()?)
}

impl Store {
    pub(super) fn execute(&self, sql: &str, params: impl IntoParams) -> Result<u64> {
        Ok(self.block_on(self.conn.execute(sql, params))?)
    }

    pub(super) fn query_all<T>(
        &self,
        sql: &str,
        params: impl IntoParams,
        map: fn(&Row) -> Result<T>,
    ) -> Result<Vec<T>> {
        self.block_on(async {
            let mut rows = self.conn.query(sql, params).await?;
            let mut mapped = Vec::new();
            while let Some(row) = rows.next().await? {
                mapped.push(map(&row)?);
            }
            Ok(mapped)
        })
    }

    pub(super) fn query_optional<T>(
        &self,
        sql: &str,
        params: impl IntoParams,
        map: fn(&Row) -> Result<T>,
    ) -> Result<Option<T>> {
        self.block_on(async {
            let mut rows = self.conn.query(sql, params).await?;
            rows.next().await?.map(|row| map(&row)).transpose()
        })
    }
}
