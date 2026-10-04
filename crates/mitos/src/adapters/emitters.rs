use anyhow::Result;
use serde_json::Value;

use crate::ports::EventSink;
use crate::wire::events::AdapterEvent;

/// Forwards events until a terminal one, dropping anything after it, and
/// remembers the latest native session an event carried.
pub struct Emitter<'a, 'b> {
    on_event: &'a mut EventSink<'b>,
    native_session: Option<Value>,
    finished: bool,
}

impl<'a, 'b> Emitter<'a, 'b> {
    pub fn new(on_event: &'a mut EventSink<'b>) -> Self {
        Self {
            on_event,
            native_session: None,
            finished: false,
        }
    }

    /// Runs `turn` against a fresh emitter and returns the latest native session.
    pub fn collect(
        on_event: &'a mut EventSink<'b>,
        turn: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<Option<Value>> {
        let mut emitter = Self::new(on_event);
        turn(&mut emitter)?;
        Ok(emitter.into_native_session())
    }

    pub fn emit(&mut self, event: AdapterEvent) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        if event.native_session.is_some() {
            self.native_session.clone_from(&event.native_session);
        }
        self.finished = event.is_terminal();
        (self.on_event)(event)
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn native_session(&self) -> Option<&Value> {
        self.native_session.as_ref()
    }

    pub fn into_native_session(self) -> Option<Value> {
        self.native_session
    }
}
