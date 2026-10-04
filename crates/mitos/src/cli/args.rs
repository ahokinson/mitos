use std::path::PathBuf;

use clap::{Args, Subcommand};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};

use crate::domain::{CompactMode, HarnessKind, RequestId, ThreadId, ThreadMode, TurnId};
use crate::service::ThreadNotes;

fn harness_candidates() -> ArgValueCandidates {
    ArgValueCandidates::new(|| {
        HarnessKind::ALL
            .into_iter()
            .map(|harness| CompletionCandidate::new(harness.as_str()))
            .collect::<Vec<_>>()
    })
}

#[derive(Debug, Args)]
pub struct NoteArgs {
    #[arg(long)]
    note: Option<String>,
    #[arg(long = "decision")]
    decisions: Vec<String>,
    #[arg(long = "question")]
    questions: Vec<String>,
}

impl From<NoteArgs> for ThreadNotes {
    fn from(args: NoteArgs) -> Self {
        Self {
            note: args.note,
            decisions: args.decisions,
            questions: args.questions,
        }
    }
}

#[derive(Debug, Args)]
pub struct EnterArgs {
    #[arg(add = harness_candidates())]
    pub(super) harness: HarnessKind,
    #[arg(long)]
    pub(super) thread: ThreadId,
    #[command(flatten)]
    pub(super) notes: NoteArgs,
    #[arg(long)]
    pub(super) native_session: Option<String>,
}

#[derive(Debug, Args)]
pub struct HookArgs {
    #[arg(add = harness_candidates())]
    pub(super) harness: String,
    #[arg(long)]
    pub(super) event: Option<String>,
    /// Echo stdin to stdout unchanged, so the hook can sit in front of another
    /// command in a pipe, such as a status line.
    #[arg(long)]
    pub(super) passthrough: bool,
}

/// Read-only views of stored state for frontends; every view prints JSON.
#[derive(Debug, Subcommand)]
pub enum ViewCommand {
    /// A workspace's threads, newest first, with their opening messages.
    Threads {
        #[arg(long)]
        workspace_key: String,
    },
    /// The user messages sent in a workspace, newest first, each text once.
    History {
        #[arg(long)]
        workspace_key: String,
        #[arg(long)]
        limit: usize,
    },
    /// Whether a thread has recorded nothing beyond its own setup events.
    Empty { id: ThreadId },
    /// A thread's latest context and cumulative usage under a harness, or null.
    Usage {
        id: ThreadId,
        #[arg(long, add = harness_candidates())]
        harness: HarnessKind,
    },
}

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Show whether each harness's hook is installed, approved, and reporting.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Install the hooks into the harnesses' own config, backing each file up first.
    Init {
        /// Limit to these harnesses; every harness when omitted.
        #[arg(long = "harness", add = harness_candidates())]
        harnesses: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ThreadCommand {
    New {
        #[arg(long, add = harness_candidates())]
        harness: Option<HarnessKind>,
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Send {
        id: ThreadId,
        #[arg(long)]
        message: String,
    },
    Sync {
        id: ThreadId,
        #[arg(long, default_value_t = 0)]
        since: i64,
        #[arg(long)]
        json: bool,
    },
    Reassign {
        id: ThreadId,
        #[arg(long = "to", add = harness_candidates())]
        to: HarnessKind,
    },
    /// Rebuild the thread's native session from a trimmed or summarized context.
    Compact {
        id: ThreadId,
        #[arg(long, default_value = "mechanical")]
        mode: CompactMode,
    },
    Attach {
        id: ThreadId,
    },
    Archive {
        id: ThreadId,
    },
    /// Permanently removes the thread and all its rows; unlike `archive`, not recoverable.
    Delete {
        id: ThreadId,
    },
    Note {
        id: ThreadId,
        #[command(flatten)]
        notes: NoteArgs,
    },
    Drive {
        id: ThreadId,
        #[arg(long)]
        turn: TurnId,
    },
    /// Set the thread's mode; takes effect on the next turn.
    Mode {
        id: ThreadId,
        mode: ThreadMode,
    },
    /// Answer a pending harness request; the response is JSON or plain text.
    Answer {
        id: ThreadId,
        #[arg(long = "request")]
        request: RequestId,
        #[arg(long, conflicts_with_all = ["deny", "text", "response"])]
        approve: bool,
        /// With `--text`, the reason given to the harness.
        #[arg(long, conflicts_with_all = ["approve", "response"])]
        deny: bool,
        #[arg(long, conflicts_with_all = ["approve", "response"])]
        text: Option<String>,
        /// The raw response, for a harness whose shape the flags don't cover.
        #[arg(long, conflicts_with_all = ["approve", "deny", "text"])]
        response: Option<String>,
    },
    /// List the thread's pending harness requests.
    Requests {
        id: ThreadId,
        #[arg(long)]
        json: bool,
    },
}
