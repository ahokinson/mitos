use serde::{Deserialize, Serialize};

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub root: String,
    pub git_dir: Option<String>,
    pub workspace_key: String,
    pub created_at: String,
    pub updated_at: String,
}
