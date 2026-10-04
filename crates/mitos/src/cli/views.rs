use anyhow::Result;

use super::args::ViewCommand;
use super::outputs::emit_json;
use crate::service::ThreadService;

pub fn dispatch(service: &ThreadService<'_>, command: ViewCommand) -> Result<()> {
    match command {
        ViewCommand::Threads { workspace_key } => {
            emit_json(&service.thread_summaries(&workspace_key)?)
        }
        ViewCommand::History {
            workspace_key,
            limit,
        } => emit_json(&service.recent_user_messages(&workspace_key, limit)?),
        ViewCommand::Empty { id } => emit_json(&service.is_thread_empty(&id)?),
        ViewCommand::Usage { id, harness } => emit_json(&service.latest_usage(&id, &harness)?),
    }
}
