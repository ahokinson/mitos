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
mod views;

use crate::config::{HandoffLimits, handoff_limits};
use crate::handoff::HandoffRenderer;
use crate::store::Store;

pub use notes::ThreadNotes;
pub use views::EventView;

pub struct ThreadService<'a> {
    store: &'a Store,
    renderer: &'a dyn HandoffRenderer,
    handoff_limits: HandoffLimits,
}

impl<'a> ThreadService<'a> {
    pub fn new(store: &'a Store, renderer: &'a dyn HandoffRenderer) -> Self {
        Self {
            store,
            renderer,
            handoff_limits: handoff_limits(store.config_root()),
        }
    }
}
