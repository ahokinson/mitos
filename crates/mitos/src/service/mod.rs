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

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::config::{HandoffLimits, Plan, configured_plans, handoff_limits};
use crate::handoff::HandoffRenderer;
use crate::ipc::answers;
use crate::store::Store;

pub use notes::ThreadNotes;
pub use threads::NativeTarget;
pub use views::EventView;

pub struct ThreadService<'a> {
    store: &'a Store,
    renderer: &'a dyn HandoffRenderer,
    answer_delivery: &'a AnswerDelivery<'a>,
    handoff_limits: HandoffLimits,
    plans: Vec<Plan>,
}

type AnswerDelivery<'a> = dyn Fn(&Path, &Value) -> Result<()> + 'a;

impl<'a> ThreadService<'a> {
    pub fn new(store: &'a Store, renderer: &'a dyn HandoffRenderer) -> Self {
        Self::from_answer_delivery(store, renderer, &answers::send_answer)
    }

    fn from_answer_delivery(
        store: &'a Store,
        renderer: &'a dyn HandoffRenderer,
        answer_delivery: &'a AnswerDelivery<'a>,
    ) -> Self {
        Self {
            store,
            renderer,
            answer_delivery,
            handoff_limits: handoff_limits(store.config_root()),
            plans: configured_plans(store.config_root()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_answer_delivery<F>(
        store: &'a Store,
        renderer: &'a dyn HandoffRenderer,
        answer_delivery: &'a F,
    ) -> Self
    where
        F: Fn(&Path, &Value) -> Result<()>,
    {
        Self::from_answer_delivery(store, renderer, answer_delivery)
    }
}
