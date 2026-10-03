mod bounds;
mod renderers;

use anyhow::Result;
use serde::Serialize;

use crate::domain::ThreadEvent;

pub use bounds::bounded_context;
pub use renderers::DeterministicRenderer;

#[derive(Clone, Debug, Serialize)]
pub struct HandoffFacts {
    pub thread_id: String,
    pub events: Vec<ThreadEvent>,
}

pub trait HandoffRenderer {
    fn render(&self, facts: &HandoffFacts) -> Result<String>;
}
