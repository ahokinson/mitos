use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

pub fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    emit_json_to(&mut io::stdout().lock(), value)
}

pub fn emit<T: Serialize>(json: bool, value: T, text: impl FnOnce(T)) -> Result<()> {
    emit_to(&mut io::stdout().lock(), json, value, text)
}

fn emit_json_to<T: Serialize>(output: &mut impl Write, value: &T) -> Result<()> {
    writeln!(output, "{}", serde_json::to_string(value)?)?;
    Ok(())
}

fn emit_to<T: Serialize>(
    output: &mut impl Write,
    json: bool,
    value: T,
    text: impl FnOnce(T),
) -> Result<()> {
    if json {
        emit_json_to(output, &value)?;
    } else {
        text(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_output_is_one_serialized_line() {
        let mut output = Vec::new();

        emit_json_to(&mut output, &serde_json::json!({ "thread": "thread-1" })).unwrap();

        assert_eq!(output, b"{\"thread\":\"thread-1\"}\n");
    }

    #[test]
    fn emit_uses_json_when_requested() {
        let mut output = Vec::new();

        emit_to(&mut output, true, "ready", |_| {
            panic!("text output must not run")
        })
        .unwrap();

        assert_eq!(output, b"\"ready\"\n");
    }

    #[test]
    fn emit_hands_values_to_the_text_formatter() {
        let mut output = Vec::new();
        let mut rendered = None;

        emit_to(&mut output, false, String::from("ready"), |value| {
            rendered = Some(value);
        })
        .unwrap();

        assert!(output.is_empty());
        assert_eq!(rendered.as_deref(), Some("ready"));
    }
}
