use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// The peer closed its output while a request was still waiting.
#[derive(Debug, thiserror::Error)]
#[error("rpc peer exited")]
pub struct PeerExited;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    Text(String),
}

impl RpcId {
    pub fn to_text(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.clone(),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Self::Number(number) => json!(number),
            Self::Text(text) => json!(text),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Inbound {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: RpcId,
        method: String,
        params: Value,
    },
}

enum Message {
    Inbound(Inbound),
    Response {
        id: RpcId,
        outcome: Result<Value, String>,
    },
}

type Sink = Box<dyn Write + Send>;

/// The write half of a peer. Cloneable, so another thread can answer
/// server-initiated requests while the owner is blocked in `call`.
#[derive(Clone)]
pub struct RpcWriter {
    sink: Arc<Mutex<Option<Sink>>>,
    versioned: bool,
}

impl RpcWriter {
    fn new(sink: impl Write + Send + 'static, versioned: bool) -> Self {
        Self {
            sink: Arc::new(Mutex::new(Some(Box::new(sink)))),
            versioned,
        }
    }

    fn send(&self, fields: Map<String, Value>) -> Result<()> {
        let mut message = Map::new();
        if self.versioned {
            message.insert("jsonrpc".into(), json!("2.0"));
        }
        message.extend(fields);
        let mut guard = self.sink.lock().expect("rpc writer lock poisoned");
        let Some(sink) = guard.as_mut() else {
            bail!("rpc connection is closed");
        };
        writeln!(sink, "{}", Value::Object(message))?;
        sink.flush()?;
        Ok(())
    }

    pub fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        self.send(call_fields(None, method, params))
    }

    pub fn respond(&self, id: &RpcId, result: Value) -> Result<()> {
        self.send(fields([("id", id.to_value()), ("result", result)]))
    }

    pub fn respond_error(&self, id: &RpcId, code: i64, message: &str) -> Result<()> {
        self.send(fields([
            ("id", id.to_value()),
            ("error", json!({ "code": code, "message": message })),
        ]))
    }

    /// Drops the sink, which closes the peer's stdin.
    pub fn close(&self) {
        self.sink.lock().expect("rpc writer lock poisoned").take();
    }
}

/// Line-oriented JSON-RPC peer over stdio. Messages omit the `jsonrpc` field
/// unless `versioned`. A reader thread forwards lines; everything else runs on
/// the caller's thread, one outstanding request at a time.
pub struct RpcPeer {
    writer: RpcWriter,
    lines: Receiver<String>,
    next_id: i64,
}

impl RpcPeer {
    pub fn new(
        sink: impl Write + Send + 'static,
        source: impl BufRead + Send + 'static,
        versioned: bool,
    ) -> Self {
        let (forward, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in source.lines().map_while(Result::ok) {
                if forward.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            writer: RpcWriter::new(sink, versioned),
            lines,
            next_id: 1,
        }
    }

    pub fn writer(&self) -> RpcWriter {
        self.writer.clone()
    }

    /// Sends a request and waits for its response, handing everything else
    /// the peer sends in the meantime to `on_inbound`.
    pub fn call(
        &mut self,
        method: &str,
        params: Option<Value>,
        on_inbound: &mut dyn FnMut(Inbound) -> Result<()>,
    ) -> Result<Value> {
        let id = RpcId::Number(self.next_id);
        self.next_id += 1;
        self.writer
            .send(call_fields(Some(id.to_value()), method, params))?;
        loop {
            match self.next_message() {
                None => return Err(PeerExited.into()),
                Some(Message::Inbound(inbound)) => on_inbound(inbound)?,
                Some(Message::Response {
                    id: response_id,
                    outcome,
                }) if response_id == id => return outcome.map_err(|message| anyhow!(message)),
                Some(Message::Response { .. }) => {}
            }
        }
    }

    /// Hands inbound traffic to `on_inbound` until the peer closes or `done`
    /// reports true after a message.
    pub fn pump(
        &mut self,
        on_inbound: &mut dyn FnMut(Inbound) -> Result<()>,
        done: &mut dyn FnMut() -> bool,
    ) -> Result<()> {
        if done() {
            return Ok(());
        }
        while let Some(message) = self.next_message() {
            if let Message::Inbound(inbound) = message {
                on_inbound(inbound)?;
            }
            if done() {
                break;
            }
        }
        Ok(())
    }

    fn next_message(&mut self) -> Option<Message> {
        while let Ok(line) = self.lines.recv() {
            if let Some(message) = parse(&line) {
                return Some(message);
            }
        }
        None
    }
}

fn fields<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// A request (with `id`) or notification; `params` is omitted when absent.
fn call_fields(id: Option<Value>, method: &str, params: Option<Value>) -> Map<String, Value> {
    let mut message = Map::new();
    if let Some(id) = id {
        message.insert("id".into(), id);
    }
    message.insert("method".into(), json!(method));
    if let Some(params) = params {
        message.insert("params".into(), params);
    }
    message
}

fn parse(line: &str) -> Option<Message> {
    let Value::Object(mut message) = serde_json::from_str(line).ok()? else {
        return None;
    };
    let id = message
        .get("id")
        .and_then(|id| serde_json::from_value::<RpcId>(id.clone()).ok());
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        let method = method.to_string();
        let params = message.remove("params").unwrap_or(Value::Null);
        return Some(Message::Inbound(match id {
            Some(id) => Inbound::Request { id, method, params },
            None => Inbound::Notification { method, params },
        }));
    }
    let id = id?;
    let outcome = match message.get("error") {
        Some(error) if !error.is_null() => Err(error_text(error)),
        _ => Ok(message.remove("result").unwrap_or(Value::Null)),
    };
    Some(Message::Response { id, outcome })
}

/// The message, plus the server's `data.details` when it adds to it.
fn error_text(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("rpc request failed");
    match error
        .pointer("/data/details")
        .and_then(Value::as_str)
        .filter(|details| !details.is_empty())
    {
        Some(details) => format!("{message}: {details}"),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn messages(&self) -> Vec<Value> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
    }

    fn peer(incoming: &str, versioned: bool) -> (RpcPeer, Captured) {
        let captured = Captured::default();
        let source = Cursor::new(incoming.as_bytes().to_vec());
        (RpcPeer::new(captured.clone(), source, versioned), captured)
    }

    #[test]
    fn a_response_settles_the_matching_request() {
        let (mut rpc, captured) = peer("{\"id\":1,\"result\":{\"ok\":true}}\n", false);
        let result = rpc
            .call("initialize", Some(json!({ "a": 1 })), &mut |_| Ok(()))
            .unwrap();
        assert_eq!(result, json!({ "ok": true }));
        assert_eq!(
            captured.messages(),
            [json!({ "id": 1, "method": "initialize", "params": { "a": 1 } })]
        );
    }

    #[test]
    fn an_error_response_fails_the_request() {
        let (mut rpc, _) = peer(
            "{\"id\":1,\"error\":{\"code\":-1,\"message\":\"bad\"}}\n",
            false,
        );
        let error = rpc.call("thread/start", None, &mut |_| Ok(())).unwrap_err();
        assert_eq!(error.to_string(), "bad");
    }

    #[test]
    fn an_errors_data_details_is_appended_to_its_message() {
        let (mut rpc, _) = peer(
            "{\"id\":1,\"error\":{\"code\":-32603,\"message\":\"Internal error\",\"data\":{\"details\":\"No LLM provider configured\"}}}\n",
            false,
        );
        let error = rpc.call("session/new", None, &mut |_| Ok(())).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Internal error: No LLM provider configured"
        );
    }

    #[test]
    fn notifications_and_server_requests_reach_the_handler_while_waiting() {
        let (mut rpc, _) = peer(
            concat!(
                "{\"method\":\"turn/started\",\"params\":{\"x\":1}}\n",
                "not json\n",
                "{\"id\":\"s1\",\"method\":\"item/fileChange/requestApproval\",\"params\":{}}\n",
                "{\"id\":99,\"result\":1}\n",
                "{\"id\":1,\"result\":null}\n",
            ),
            false,
        );
        let mut seen = Vec::new();
        rpc.call("x", None, &mut |inbound| {
            seen.push(inbound);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            [
                Inbound::Notification {
                    method: "turn/started".into(),
                    params: json!({ "x": 1 })
                },
                Inbound::Request {
                    id: RpcId::Text("s1".into()),
                    method: "item/fileChange/requestApproval".into(),
                    params: json!({})
                },
            ]
        );
    }

    #[test]
    fn respond_and_respond_error_reuse_the_servers_id() {
        let (rpc, captured) = peer("", false);
        let writer = rpc.writer();
        writer
            .respond(&RpcId::Text("s1".into()), json!({ "decision": "accept" }))
            .unwrap();
        writer
            .respond_error(&RpcId::Number(2), -32601, "nope")
            .unwrap();
        assert_eq!(
            captured.messages(),
            [
                json!({ "id": "s1", "result": { "decision": "accept" } }),
                json!({ "id": 2, "error": { "code": -32601, "message": "nope" } }),
            ]
        );
    }

    #[test]
    fn a_closed_connection_fails_the_pending_request() {
        let (mut rpc, _) = peer("", false);
        let error = rpc.call("x", None, &mut |_| Ok(())).unwrap_err();
        assert_eq!(error.to_string(), "rpc peer exited");
    }

    #[test]
    fn a_versioned_peer_stamps_jsonrpc_on_every_message() {
        let (mut rpc, captured) = peer("{\"id\":1,\"result\":null}\n", true);
        rpc.call("initialize", None, &mut |_| Ok(())).unwrap();
        let writer = rpc.writer();
        writer.notify("initialized", None).unwrap();
        writer
            .respond(&RpcId::Number(7), json!({ "ok": true }))
            .unwrap();
        let versions: Vec<_> = captured
            .messages()
            .iter()
            .map(|message| message["jsonrpc"].clone())
            .collect();
        assert_eq!(versions, [json!("2.0"), json!("2.0"), json!("2.0")]);
    }

    #[test]
    fn params_are_omitted_rather_than_sent_as_null() {
        let (mut rpc, captured) = peer("{\"id\":1,\"result\":null}\n", false);
        rpc.call("initialize", None, &mut |_| Ok(())).unwrap();
        rpc.writer().notify("initialized", None).unwrap();
        rpc.writer()
            .notify("note", Some(json!({ "x": 1 })))
            .unwrap();
        assert_eq!(
            captured.messages(),
            [
                json!({ "id": 1, "method": "initialize" }),
                json!({ "method": "initialized" }),
                json!({ "method": "note", "params": { "x": 1 } }),
            ]
        );
    }

    #[test]
    fn a_closed_writer_refuses_further_messages() {
        let (rpc, _) = peer("", false);
        let writer = rpc.writer();
        writer.close();
        assert!(writer.notify("x", None).is_err());
    }

    #[test]
    fn pump_stops_once_done_or_when_the_peer_closes() {
        let (mut rpc, _) = peer(
            "{\"method\":\"a\"}\n{\"method\":\"b\"}\n{\"method\":\"c\"}\n",
            false,
        );
        let mut methods = Vec::new();
        let finished = std::cell::Cell::new(false);
        rpc.pump(
            &mut |inbound| {
                if let Inbound::Notification { method, .. } = inbound {
                    finished.set(method == "b");
                    methods.push(method);
                }
                Ok(())
            },
            &mut || finished.get(),
        )
        .unwrap();
        assert_eq!(methods, ["a", "b"]);

        let (mut closed, _) = peer("{\"method\":\"a\"}\n", false);
        let mut count = 0;
        closed
            .pump(
                &mut |_| {
                    count += 1;
                    Ok(())
                },
                &mut || false,
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
