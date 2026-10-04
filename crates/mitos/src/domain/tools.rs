string_enum! {
    enum ToolKind("tool kind") {
        Shell => "shell",
        Read => "read",
        Edit => "edit",
        Write => "write",
        Search => "search",
        Fetch => "fetch",
        Agent => "agent",
        Todo => "todo",
        Other => "other",
    }
}

impl ToolKind {
    /// The kind a harness's own tool name stands for, whatever its casing.
    pub fn named(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "bash" | "execute" | "commandexecution" => Self::Shell,
            "read" | "notebookread" => Self::Read,
            "edit" | "multiedit" | "notebookedit" | "filechange" => Self::Edit,
            "write" => Self::Write,
            "grep" | "glob" | "search" => Self::Search,
            "webfetch" | "websearch" | "fetch" => Self::Fetch,
            "task" | "agent" => Self::Agent,
            "todowrite" => Self::Todo,
            _ => Self::Other,
        }
    }

    pub fn edits_files(self) -> bool {
        matches!(self, Self::Edit | Self::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::ToolKind;

    #[test]
    fn names_map_to_kinds_regardless_of_case() {
        assert_eq!(ToolKind::named("Bash"), ToolKind::Shell);
        assert_eq!(ToolKind::named("commandExecution"), ToolKind::Shell);
        assert_eq!(ToolKind::named("NotebookRead"), ToolKind::Read);
        assert_eq!(ToolKind::named("MultiEdit"), ToolKind::Edit);
        assert_eq!(ToolKind::named("fileChange"), ToolKind::Edit);
        assert_eq!(ToolKind::named("Write"), ToolKind::Write);
        assert_eq!(ToolKind::named("Glob"), ToolKind::Search);
        assert_eq!(ToolKind::named("WebSearch"), ToolKind::Fetch);
        assert_eq!(ToolKind::named("Task"), ToolKind::Agent);
        assert_eq!(ToolKind::named("TodoWrite"), ToolKind::Todo);
        assert_eq!(ToolKind::named("mystery"), ToolKind::Other);
        assert_eq!(ToolKind::named(""), ToolKind::Other);
    }

    #[test]
    fn only_edit_and_write_edit_files() {
        let editing: Vec<_> = [
            ToolKind::Edit,
            ToolKind::Write,
            ToolKind::Read,
            ToolKind::Shell,
        ]
        .into_iter()
        .map(ToolKind::edits_files)
        .collect();
        assert_eq!(editing, [true, true, false, false]);
    }
}
