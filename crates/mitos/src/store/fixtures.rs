use std::path::{Path, PathBuf};

use super::Store;
use crate::domain::{Thread, id};

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
