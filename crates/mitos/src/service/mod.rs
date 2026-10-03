mod bindings;
mod entries;
mod handoffs;
mod notes;
mod requests;
#[cfg(test)]
mod tests;
mod threads;
mod turns;
mod usage;

use crate::config::handoff_max_inline_bytes;
use crate::handoff::HandoffRenderer;
use crate::store::Store;

pub use notes::ThreadNotes;

pub struct ThreadService<'a> {
    store: &'a Store,
    renderer: &'a dyn HandoffRenderer,
    handoff_max_inline_bytes: usize,
}

impl<'a> ThreadService<'a> {
    pub fn new(store: &'a Store, renderer: &'a dyn HandoffRenderer) -> Self {
        Self {
            store,
            renderer,
            handoff_max_inline_bytes: handoff_max_inline_bytes(store.config_root()),
        }
    }
}
