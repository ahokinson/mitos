use std::io::{BufRead, BufReader};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use ureq::Agent;
use ureq::config::Config;

use crate::json::string_value;

pub const SERVER_USER: &str = "mitos";

pub struct ServerAccess {
    pub base: String,
    pub password: String,
}

/// A client for one private `opencode serve`, scoped to a workspace directory.
#[derive(Clone)]
pub struct ServerClient {
    agent: Agent,
    base: String,
    directory: String,
    authorization: String,
}

impl ServerClient {
    pub fn new(access: &ServerAccess, directory: &str) -> Self {
        let config = Config::builder().http_status_as_error(false).build();
        Self {
            agent: Agent::new_with_config(config),
            base: access.base.clone(),
            directory: directory.into(),
            authorization: format!(
                "Basic {}",
                base64(format!("{SERVER_USER}:{}", access.password).as_bytes())
            ),
        }
    }

    pub fn create_session(&self) -> Result<String> {
        let created = self.post("/session", &json!({}))?;
        match string_value(created.get("id")) {
            Some(id) => Ok(id.to_string()),
            None => bail!("opencode did not return a session id"),
        }
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        let path = format!("/session/{id}");
        self.agent
            .delete(self.url(&path))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .call()
            .with_context(|| format!("opencode {path} failed"))?;
        Ok(())
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let mut response = self
            .agent
            .post(self.url(path))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .content_type("application/json")
            .send(body.to_string())
            .with_context(|| format!("opencode {path} failed"))?;
        let status = response.status();
        let text = response.body_mut().read_to_string()?;
        if !status.is_success() {
            bail!(
                "opencode {path} failed: {} {}",
                status.as_u16(),
                text.trim()
            );
        }
        if text.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Returns once the stream is open; the events follow.
    pub fn subscribe(&self) -> Result<SseEvents> {
        let response = self
            .agent
            .get(self.url("/event"))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .call()
            .context("opencode /event failed")?;
        let status = response.status();
        if !status.is_success() {
            bail!("opencode /event failed: {}", status.as_u16());
        }
        Ok(SseEvents::new(BufReader::new(
            response.into_body().into_reader(),
        )))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// Each SSE `data:` payload as JSON. A malformed frame is skipped rather than
/// ending the turn; the stream ends at EOF or a read error.
pub struct SseEvents {
    reader: Box<dyn BufRead + Send>,
}

impl SseEvents {
    pub fn new(reader: impl BufRead + Send + 'static) -> Self {
        Self {
            reader: Box::new(reader),
        }
    }
}

impl Iterator for SseEvents {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        let mut data: Vec<String> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            let text = line.trim_end_matches(['\r', '\n']);
            if text.is_empty() {
                if data.is_empty() {
                    continue;
                }
                let payload = data.join("\n");
                data.clear();
                if let Ok(value) = serde_json::from_str(&payload) {
                    return Some(value);
                }
            } else if let Some(rest) = text.strip_prefix("data:") {
                data.push(rest.trim_start().to_string());
            }
        }
    }
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                encoded.push(ALPHABET[(group >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"mitos:secret"), "bWl0b3M6c2VjcmV0");
    }

    #[test]
    fn sse_yields_each_data_payload_and_skips_malformed_frames() {
        let stream = concat!(
            ": comment\n",
            "data: {\"a\":1}\n\n",
            "data: not json\n\n",
            "event: ignored\n",
            "data: {\"b\":\ndata: 2}\r\n\r\n",
            "\n",
            "data: {\"c\":3}\n\n",
            "data: {\"unterminated\":true}\n",
        );
        let events: Vec<Value> = SseEvents::new(Cursor::new(stream.as_bytes().to_vec())).collect();
        assert_eq!(
            events,
            [json!({ "a": 1 }), json!({ "b": 2 }), json!({ "c": 3 })]
        );
    }
}
