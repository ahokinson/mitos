use crate::wire::events::AdapterEvent;

pub fn kinds_of(events: &[AdapterEvent]) -> Vec<&str> {
    events.iter().map(|event| event.event.as_str()).collect()
}
