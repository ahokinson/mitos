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

    pub fn native_session(&self) -> Option<&Value> {
        self.native_session.as_ref()
    }

    pub fn into_native_session(self) -> Option<Value> {
        self.native_session
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::wire::events::kinds;

    #[test]
    fn records_the_latest_native_session_and_drops_events_after_completion() {
        let mut events = Vec::new();
        let mut sink = |event| {
            events.push(event);
            Ok(())
        };
        let mut emitter = Emitter::new(&mut sink);

        emitter
            .emit(AdapterEvent {
                native_session: Some(json!("session-1")),
                ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
            })
            .unwrap();
        emitter
            .emit(AdapterEvent::new(kinds::TURN_COMPLETE))
            .unwrap();
        emitter.emit(AdapterEvent::new(kinds::STATUS)).unwrap();

        assert_eq!(emitter.native_session(), Some(&json!("session-1")));
        assert_eq!(emitter.into_native_session(), Some(json!("session-1")));
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn collect_returns_the_session_from_a_successful_turn() {
        let mut sink = |_| Ok(());

        let session = Emitter::collect(&mut sink, |emitter| {
            emitter.emit(AdapterEvent {
                native_session: Some(json!("session-2")),
                ..AdapterEvent::new(kinds::NATIVE_SESSION_UPDATE)
            })
        })
        .unwrap();

        assert_eq!(session, Some(json!("session-2")));
    }
}
