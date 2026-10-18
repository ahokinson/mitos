use anyhow::Result;

use super::args::ViewCommand;
use super::outputs::emit_json;
use crate::harnesses::Adapters;
use crate::ports::HarnessAdapter;
use crate::service::ThreadService;
use crate::wire::requests::MiningRequest;

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
        ViewCommand::Usage { id, harness } => emit_json(&service.latest_usage(&id, harness)?),
        ViewCommand::Mining {
            harness,
            directory,
            since_days,
            top,
        } => {
            let request = MiningRequest::new(directory, since_days, top);
            let report = match harness {
                Some(kind) => Adapters::mine_one(kind, &request),
                None => Adapters.mine_history(&request),
            };
            emit_json(&report.truncated(request.top))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::domain::HarnessKind;
    use crate::handoff::DeterministicRenderer;
    use crate::store::Store;

    struct Dispatch {
        root: PathBuf,
        store: Store,
    }

    impl Dispatch {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("mitos-cli-views-{}", crate::domain::id()));
            std::fs::create_dir_all(&root).unwrap();
            Self {
                root: root.clone(),
                store: Store::new(Some(root.clone())).unwrap(),
            }
        }

        fn dispatch(&self, command: ViewCommand) {
            let service = ThreadService::new(&self.store, &DeterministicRenderer);
            dispatch(&service, command).unwrap();
        }
    }

    impl Drop for Dispatch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn mining(
        harness: Option<HarnessKind>,
        directory: Option<&str>,
        since_days: Option<i64>,
    ) -> ViewCommand {
        ViewCommand::Mining {
            harness,
            directory: directory.map(str::to_string),
            since_days,
            top: 2,
        }
    }

    #[test]
    fn mining_dispatch_reports_without_any_harness_history() {
        let dispatch = Dispatch::new();
        let empty_data_home = dispatch.root.join("no-harness-histories");
        std::fs::create_dir_all(&empty_data_home).unwrap();
        let original = std::env::var_os("XDG_DATA_HOME");
        let original_claude = std::env::var_os("CLAUDE_CONFIG_DIR");
        let original_codex = std::env::var_os("CODEX_HOME");
        // SAFETY: no other test reads these variables while this one runs.
        unsafe {
            std::env::set_var("XDG_DATA_HOME", &empty_data_home);
            std::env::set_var("CLAUDE_CONFIG_DIR", &empty_data_home);
            std::env::set_var("CODEX_HOME", &empty_data_home);
        }
        dispatch.dispatch(mining(None, None, None));
        dispatch.dispatch(mining(None, Some("/workspace"), Some(7)));
        dispatch.dispatch(mining(Some(HarnessKind::Claude), None, None));
        // SAFETY: restored before anything else can observe the swap.
        unsafe {
            match original {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match original_claude {
                Some(value) => std::env::set_var("CLAUDE_CONFIG_DIR", value),
                None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
            }
            match original_codex {
                Some(value) => std::env::set_var("CODEX_HOME", value),
                None => std::env::remove_var("CODEX_HOME"),
            }
        }
    }

    #[test]
    fn every_other_view_reports_on_an_empty_store() {
        let dispatch = Dispatch::new();
        let id: crate::domain::ThreadId = "th_missing".parse().unwrap();
        dispatch.dispatch(ViewCommand::Threads {
            workspace_key: "ws".into(),
        });
        dispatch.dispatch(ViewCommand::History {
            workspace_key: "ws".into(),
            limit: 5,
        });
        dispatch.dispatch(ViewCommand::Empty { id: id.clone() });
        dispatch.dispatch(ViewCommand::Usage {
            id,
            harness: HarnessKind::OpenCode,
        });
    }
}
