mod bounds;
mod extracts;
mod renderers;

use anyhow::Result;
use serde::Serialize;

use crate::domain::{ThreadEvent, ThreadId};

pub use bounds::bounded_context;
pub use extracts::{contents_of, files_touched};
pub use renderers::{DeterministicRenderer, PREAMBLE_PREFIX};

#[derive(Clone, Debug, Serialize)]
pub struct HandoffFacts {
    pub thread_id: ThreadId,
    pub events: Vec<ThreadEvent>,
    pub files_touched: Vec<String>,
}

pub trait HandoffRenderer {
    /// Drops the least useful material to approach `max_bytes`; the caller
    /// still bounds whatever comes back.
    fn render(&self, facts: &HandoffFacts, max_bytes: usize) -> Result<String>;
}
