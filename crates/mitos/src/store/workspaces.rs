use std::path::Path;

use anyhow::{Context, Result};
use libsql::{Row, params};

use super::Store;
use crate::domain::{Workspace, id, now};

const COLUMNS: &str = "id, root, git_dir, workspace_key, created_at, updated_at";

fn workspace_from_row(row: &Row) -> Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        root: row.get(1)?,
        git_dir: row.get(2)?,
        workspace_key: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

impl Store {
    pub fn workspace_for(
        &self,
        root: &Path,
        git_dir: Option<&Path>,
        workspace_key: &str,
    ) -> Result<Workspace> {
        if let Some(workspace) = self.find_workspace_by_key(workspace_key)? {
            return Ok(workspace);
        }
        let workspace = Workspace {
            id: id(),
            root: root.to_string_lossy().into_owned(),
            git_dir: git_dir.map(|path| path.to_string_lossy().into_owned()),
            workspace_key: workspace_key.to_string(),
            created_at: now(),
            updated_at: now(),
        };
        let inserted = self.execute(
            &format!("INSERT INTO workspaces ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
            params![
                workspace.id.clone(),
                workspace.root.clone(),
                workspace.git_dir.clone(),
                workspace.workspace_key.clone(),
                workspace.created_at.clone(),
                workspace.updated_at.clone()
            ],
        );
        match inserted {
            Ok(_) => Ok(workspace),
            Err(error) => self.find_workspace_by_key(workspace_key)?.ok_or(error),
        }
    }

    pub fn find_workspace_by_key(&self, workspace_key: &str) -> Result<Option<Workspace>> {
        self.query_optional(
            &format!("SELECT {COLUMNS} FROM workspaces WHERE workspace_key = ?1"),
            params![workspace_key],
            workspace_from_row,
        )
    }

    pub fn get_workspace(&self, id: &str) -> Result<Workspace> {
        self.query_optional(
            &format!("SELECT {COLUMNS} FROM workspaces WHERE id = ?1"),
            params![id],
            workspace_from_row,
        )?
        .context("no such workspace")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::store::fixtures::test_store;

    #[test]
    fn workspace_for_is_idempotent_for_the_same_key() {
        let (store, root) = test_store();
        let first = store
            .workspace_for(Path::new("/workspace"), None, "key-a")
            .unwrap();
        let second = store
            .workspace_for(Path::new("/workspace"), None, "key-a")
            .unwrap();
        assert_eq!(first.id, second.id);
        fs::remove_dir_all(root).unwrap();
    }
}
