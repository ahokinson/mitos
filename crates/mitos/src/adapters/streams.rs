use std::io::BufRead;

use anyhow::{Context, Result};
use serde_json::Value;

use super::emitters::Emitter;
use crate::ports::EventSink;
use crate::wire::events::AdapterEvent;

pub fn dispatch_ndjson(
    reader: impl BufRead,
    on_event: &mut EventSink<'_>,
) -> Result<Option<Value>> {
    let mut emitter = Emitter::new(on_event);
    for line in reader.lines() {
        let line = line.context("could not read adapter output")?;
        if line.trim().is_empty() {
            continue;
        }
        let event: AdapterEvent = serde_json::from_str(&line)
            .with_context(|| format!("adapter emitted invalid event JSON: {line}"))?;
        emitter.emit(event)?;
        if emitter.finished() {
            break;
        }
    }
    Ok(emitter.into_native_session())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn dispatch_ndjson_stops_at_turn_complete_and_tracks_native_session() {
        let input = concat!(
            "{\"event\":\"assistant_delta\",\"content\":\"hi\"}\n",
            "{\"event\":\"native_session_update\",\"native_session\":\"abc\"}\n",
            "{\"event\":\"turn_complete\"}\n",
            "{\"event\":\"assistant_delta\",\"content\":\"should not be seen\"}\n",
        );
        let mut seen = Vec::new();
        let native_session = dispatch_ndjson(Cursor::new(input.as_bytes()), &mut |event| {
            seen.push(event.event.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            vec!["assistant_delta", "native_session_update", "turn_complete"]
        );
        assert_eq!(native_session, Some(Value::String("abc".into())));
    }

    #[test]
    fn dispatch_ndjson_stops_at_error() {
        let input = "{\"event\":\"status\",\"content\":\"starting\"}\n{\"event\":\"error\",\"content\":\"boom\"}\n{\"event\":\"status\",\"content\":\"unreachable\"}\n";
        let mut seen = Vec::new();
        dispatch_ndjson(Cursor::new(input.as_bytes()), &mut |event| {
            seen.push(event.event.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, vec!["status", "error"]);
    }
}
