#[cfg(test)]
use std::cell::RefCell;
use std::io::{BufRead, BufReader};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use ureq::Agent;
use ureq::Body;
use ureq::config::Config;
use ureq::http::Response;

use crate::json::string_value;

pub const SERVER_USER: &str = "mitos";

#[derive(Clone)]
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

#[cfg(test)]
thread_local! {
    static TEST_AGENT: RefCell<Option<Agent>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_test_agent(agent: Option<Agent>) {
    TEST_AGENT.with(|slot| *slot.borrow_mut() = agent);
}

impl ServerClient {
    pub fn new(access: &ServerAccess, directory: &str) -> Self {
        #[cfg(test)]
        if let Some(agent) = TEST_AGENT.with(|slot| slot.borrow().clone()) {
            return Self::with_agent(access, directory, agent);
        }
        let config = Config::builder().http_status_as_error(false).build();
        Self::with_agent(access, directory, Agent::new_with_config(config))
    }

    fn with_agent(access: &ServerAccess, directory: &str, agent: Agent) -> Self {
        Self {
            agent,
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
        let response = self
            .agent
            .delete(self.url(&path))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .call()
            .with_context(|| format!("opencode {path} failed"))?;
        Self::response_text(&path, response)?;
        Ok(())
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self
            .agent
            .post(self.url(path))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .content_type("application/json")
            .send(body.to_string())
            .with_context(|| format!("opencode {path} failed"))?;
        Self::json_response(path, response)
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let response = self
            .agent
            .get(self.url(path))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .call()
            .with_context(|| format!("opencode {path} failed"))?;
        Self::json_response(path, response)
    }

    fn json_response(path: &str, response: Response<Body>) -> Result<Value> {
        let text = Self::response_text(path, response)?;
        if text.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    fn response_text(path: &str, mut response: Response<Body>) -> Result<String> {
        let status = response.status();
        let text = response.body_mut().read_to_string()?;
        if !status.is_success() {
            bail!(
                "opencode {path} failed: {} {}",
                status.as_u16(),
                text.trim()
            );
        }
        Ok(text)
    }

    /// Whether the server answers `/global/health`; a dead or stale one does not.
    pub fn healthy(&self) -> bool {
        self.agent
            .get(self.url("/global/health"))
            .query("directory", &self.directory)
            .header("authorization", &self.authorization)
            .call()
            .is_ok_and(|response| response.status().is_success())
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
pub(crate) mod test_support {
    use std::collections::VecDeque;
    use std::fmt;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use super::{Agent, Config, ServerAccess, ServerClient};
    use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
    use ureq::unversioned::transport::{
        Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport,
    };

    #[derive(Debug)]
    struct TestResolver;

    impl Resolver for TestResolver {
        fn resolve(
            &self,
            _uri: &ureq::http::Uri,
            _config: &Config,
            _timeout: NextTimeout,
        ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
            let mut addresses = self.empty();
            addresses.push(SocketAddr::from(([127, 0, 0, 1], 80)));
            Ok(addresses)
        }
    }

    #[derive(Debug)]
    struct TestConnector {
        request: Arc<Mutex<Vec<u8>>>,
        responses: TestResponses,
    }

    #[derive(Debug)]
    enum TestResponses {
        Fixed(Vec<u8>),
        Queued(Arc<Mutex<VecDeque<Vec<u8>>>>),
    }

    impl Connector for TestConnector {
        type Out = TestTransport;

        fn connect(
            &self,
            details: &ConnectionDetails,
            _chained: Option<()>,
        ) -> std::result::Result<Option<Self::Out>, ureq::Error> {
            let response = match &self.responses {
                TestResponses::Fixed(response) => response.clone(),
                TestResponses::Queued(responses) => {
                    responses.lock().unwrap().pop_front().unwrap_or_else(|| {
                        http_response(
                            "500 Internal Server Error",
                            "text/plain",
                            "unexpected in-process request",
                        )
                        .into_bytes()
                    })
                }
            };
            Ok(Some(TestTransport {
                buffers: LazyBuffers::new(
                    details.config.input_buffer_size(),
                    details.config.output_buffer_size(),
                ),
                request: Arc::clone(&self.request),
                response,
            }))
        }
    }

    #[derive(Debug)]
    struct FailingConnector;

    impl Connector for FailingConnector {
        type Out = TestTransport;

        fn connect(
            &self,
            _details: &ConnectionDetails,
            _chained: Option<()>,
        ) -> std::result::Result<Option<Self::Out>, ureq::Error> {
            Err(ureq::Error::ConnectionFailed)
        }
    }

    struct TestTransport {
        buffers: LazyBuffers,
        request: Arc<Mutex<Vec<u8>>>,
        response: Vec<u8>,
    }

    impl fmt::Debug for TestTransport {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.debug_struct("TestTransport").finish()
        }
    }

    impl Transport for TestTransport {
        fn buffers(&mut self) -> &mut dyn Buffers {
            &mut self.buffers
        }

        fn transmit_output(
            &mut self,
            amount: usize,
            _timeout: NextTimeout,
        ) -> std::result::Result<(), ureq::Error> {
            let output = self.buffers.output()[..amount].to_vec();
            self.request.lock().unwrap().extend(output);
            Ok(())
        }

        fn await_input(&mut self, _timeout: NextTimeout) -> std::result::Result<bool, ureq::Error> {
            let amount = self
                .buffers
                .input_append_buf()
                .len()
                .min(self.response.len());
            if amount == 0 {
                return Ok(false);
            }
            let response: Vec<u8> = self.response.drain(..amount).collect();
            self.buffers.input_append_buf()[..amount].copy_from_slice(&response);
            self.buffers.input_appended(amount);
            Ok(true)
        }

        fn is_open(&mut self) -> bool {
            !self.response.is_empty()
        }
    }

    pub(crate) fn http_response(status: &str, content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\ncontent-length: {}\r\ncontent-type: {content_type}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(crate) fn agent_with_response(status: &str, body: &str) -> (Agent, Arc<Mutex<Vec<u8>>>) {
        let request = Arc::new(Mutex::new(Vec::new()));
        let config = Config::builder().http_status_as_error(false).build();
        let agent = Agent::with_parts(
            config,
            TestConnector {
                request: Arc::clone(&request),
                responses: TestResponses::Fixed(
                    http_response(status, "application/json", body).into_bytes(),
                ),
            },
            TestResolver,
        );
        (agent, request)
    }

    pub(crate) fn client_with_responses(
        responses: impl IntoIterator<Item = String>,
    ) -> (ServerClient, Arc<Mutex<Vec<u8>>>) {
        let request = Arc::new(Mutex::new(Vec::new()));
        let config = Config::builder().http_status_as_error(false).build();
        let agent = Agent::with_parts(
            config,
            TestConnector {
                request: Arc::clone(&request),
                responses: TestResponses::Queued(Arc::new(Mutex::new(
                    responses.into_iter().map(String::into_bytes).collect(),
                ))),
            },
            TestResolver,
        );
        let access = ServerAccess {
            base: "http://opencode.test".into(),
            password: "secret".into(),
        };
        (
            ServerClient::with_agent(&access, "/work/mitos", agent),
            request,
        )
    }

    pub(crate) fn client_with_response(
        status: &str,
        body: &str,
    ) -> (ServerClient, Arc<Mutex<Vec<u8>>>) {
        let (agent, request) = agent_with_response(status, body);
        let access = ServerAccess {
            base: "http://opencode.test".into(),
            password: "secret".into(),
        };
        (
            ServerClient::with_agent(&access, "/work/mitos", agent),
            request,
        )
    }

    pub(crate) fn client_with_connection_error() -> ServerClient {
        let config = Config::builder().http_status_as_error(false).build();
        let agent = Agent::with_parts(config, FailingConnector, TestResolver);
        let access = ServerAccess {
            base: "http://opencode.test".into(),
            password: "secret".into(),
        };
        ServerClient::with_agent(&access, "/work/mitos", agent)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::test_support::{client_with_connection_error, client_with_response};
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

    #[test]
    fn post_builds_an_authenticated_json_request_and_reads_the_response() {
        let (client, request) = client_with_response("201 Created", r#"{"id":"ses_1"}"#);

        assert_eq!(
            client.post("/session", &json!({"title": "test"})).unwrap(),
            json!({"id": "ses_1"})
        );

        let request = String::from_utf8(request.lock().unwrap().clone()).unwrap();
        assert!(request.starts_with("POST /session?directory=%2Fwork%2Fmitos HTTP/1.1\r\n"));
        assert!(request.contains("\r\nauthorization: Basic bWl0b3M6c2VjcmV0\r\n"));
        assert!(request.contains("\r\ncontent-type: application/json\r\n"));
        assert!(request.ends_with("\r\n\r\n{\"title\":\"test\"}"));
    }

    #[test]
    fn get_returns_status_errors_and_accepts_empty_success_responses() {
        let (client, _) = client_with_response("404 Not Found", "missing\n");
        let error = client.get("/session/missing").unwrap_err();
        assert_eq!(
            error.to_string(),
            "opencode /session/missing failed: 404 missing"
        );

        let (client, _) = client_with_response("204 No Content", "");
        assert_eq!(client.get("/session").unwrap(), Value::Null);
    }

    #[test]
    fn create_and_delete_session_validate_the_response() {
        let (client, _) = client_with_response("200 OK", r#"{"id":"ses_1"}"#);
        assert_eq!(client.create_session().unwrap(), "ses_1");

        let (client, _) = client_with_response("200 OK", "{}");
        assert_eq!(
            client.create_session().unwrap_err().to_string(),
            "opencode did not return a session id"
        );

        let (client, _) = client_with_response("500 Internal Server Error", "failed");
        let error = client.delete_session("ses_1").unwrap_err();
        assert_eq!(
            error.to_string(),
            "opencode /session/ses_1 failed: 500 failed"
        );
    }

    #[test]
    fn health_and_event_subscription_use_the_server_endpoints() {
        let (client, request) = client_with_response("204 No Content", "");
        assert!(client.healthy());
        let request = String::from_utf8(request.lock().unwrap().clone()).unwrap();
        assert!(request.starts_with("GET /global/health?directory=%2Fwork%2Fmitos HTTP/1.1\r\n"));

        let client = client_with_connection_error();
        assert!(!client.healthy());

        let (client, request) = client_with_response("200 OK", "data: {\"type\":\"ready\"}\n\n");
        assert_eq!(
            client.subscribe().unwrap().collect::<Vec<_>>(),
            [json!({ "type": "ready" })]
        );
        let request = String::from_utf8(request.lock().unwrap().clone()).unwrap();
        assert!(request.starts_with("GET /event?directory=%2Fwork%2Fmitos HTTP/1.1\r\n"));

        let (client, _) = client_with_response("503 Service Unavailable", "");
        let Err(error) = client.subscribe() else {
            panic!("the unavailable event stream must fail");
        };
        assert_eq!(error.to_string(), "opencode /event failed: 503");
    }
}
