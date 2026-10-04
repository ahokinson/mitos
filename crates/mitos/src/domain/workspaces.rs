use serde::{Deserialize, Serialize};

use super::{Timestamp, WorkspaceId};

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub root: String,
    pub git_dir: Option<String>,
    pub workspace_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
