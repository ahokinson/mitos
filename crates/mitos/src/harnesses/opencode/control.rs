use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::clients::ServerClient;
use super::daemon;
use crate::json::string_value;

/// Where the account's Go plan windows live; the credentials that open this
/// door are the harness's own.
const ZEN_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

/// The account's Go plan windows, as the usage endpoint reports them.
#[derive(Clone, Debug, Default)]
pub(crate) struct ZenPlan {
    pub five_hour_percent: Option<f64>,
    pub five_hour_resets_at: Option<String>,
    pub week_percent: Option<f64>,
    pub week_resets_at: Option<String>,
    pub month_percent: Option<f64>,
    pub month_resets_at: Option<String>,
}

fn client_for(workdir: &Path) -> Result<ServerClient> {
    let access = daemon::control_access(workdir)?;
    Ok(ServerClient::new(&access, &workdir.to_string_lossy()))
}

/// Stops the session's running turn; harmless when no turn is running.
pub(crate) fn abort(workdir: &Path, session: &str) -> Result<()> {
    let client = client_for(workdir)?;
    client.post(&format!("/session/{session}/abort"), &json!({}))?;
    Ok(())
}

/// Forks the session at its newest message; returns the child session id.
pub(crate) fn fork(workdir: &Path, session: &str) -> Result<String> {
    let client = client_for(workdir)?;
    let forked = client.post(&format!("/session/{session}/fork"), &json!({}))?;
    string_value(forked.get("id"))
        .map(str::to_string)
        .context("opencode did not return a forked session id")
}

/// Publishes the session; returns its share URL.
pub(crate) fn share(workdir: &Path, session: &str) -> Result<String> {
    let client = client_for(workdir)?;
    let shared = client.post(&format!("/session/{session}/share"), &json!({}))?;
    string_value(shared.pointer("/share/url"))
        .map(str::to_string)
        .context("opencode did not return a share url")
}

/// The session's file diffs: `{ file, before, after, additions, deletions }`.
pub(crate) fn diff(workdir: &Path, session: &str) -> Result<Value> {
    let client = client_for(workdir)?;
    client.get(&format!("/session/{session}/diff"))
}

/// The server's agents: `{ name, description, mode, builtIn, ... }`.
pub(crate) fn agents(workdir: &Path) -> Result<Value> {
    let client = client_for(workdir)?;
    client.get("/agent")
}

/// The server's default model per provider, plus the provider ids.
pub(crate) fn model_defaults(workdir: &Path) -> Result<Value> {
    let client = client_for(workdir)?;
    let config = client.get("/config/providers")?;
    let providers: Vec<&str> = config
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|provider| provider.get("id").and_then(Value::as_str))
        .collect();
    Ok(json!({
        "default": config.get("default").cloned().unwrap_or(Value::Null),
        "providers": providers,
    }))
}

/// The account's Go plan windows, borrowed from the harness: the key and
/// org header come back over the connection mitos already keeps with the
/// harness's server, and the call carries the same headers the harness
/// itself sends. Windows are account-wide, so they cover every machine.
/// Any failure yields `None` — the mined estimate covers the gap.
pub(crate) fn zen_usage(client: &ServerClient, session: &str) -> Option<ZenPlan> {
    zen_usage_from(client, session, ZEN_USAGE_URL)
}

fn zen_usage_from(client: &ServerClient, session: &str, url: &str) -> Option<ZenPlan> {
    let config = client.get("/config/providers").ok()?;
    let provider = config
        .get("providers")?
        .as_array()?
        .iter()
        .find(|provider| provider.get("id").and_then(Value::as_str) == Some("opencode-go"))?;
    let key = provider.pointer("/options/apiKey")?.as_str()?;
    let org = provider
        .pointer("/options/headers/x-opencode-org-id")?
        .as_str()?;
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .build(),
    );
    let mut response = agent
        .get(url)
        .header("authorization", &format!("Bearer {key}"))
        .header("user-agent", "mitos")
        .header("x-opencode-session", session)
        .header("x-opencode-org-id", org)
        .call()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = serde_json::from_str(&response.body_mut().read_to_string().ok()?).ok()?;
    let window = |name: &str| {
        let node = body.pointer(&format!("/usage/{name}"))?;
        Some((
            node.get("percent").and_then(Value::as_f64),
            string_value(node.get("resetsAt")).map(str::to_string),
        ))
    };
    let (five_hour_percent, five_hour_resets_at) = window("rolling")?;
    let (week_percent, week_resets_at) = window("weekly")?;
    let (month_percent, month_resets_at) = window("monthly")?;
    Some(ZenPlan {
        five_hour_percent,
        five_hour_resets_at,
        week_percent,
        week_resets_at,
        month_percent,
        month_resets_at,
    })
}

/// Runs an `OpenCode` slash command in the session; returns the reply's text.
pub(crate) fn run_command(
    workdir: &Path,
    session: &str,
    command: &str,
    arguments: &str,
    agent: Option<&str>,
    model: Option<&str>,
) -> Result<String> {
    let client = client_for(workdir)?;
    let mut body = json!({ "command": command, "arguments": arguments });
    if let Some(agent) = agent {
        body["agent"] = json!(agent);
    }
    if let Some(model) = model {
        body["model"] = json!(model);
    }
    let reply = client.post(&format!("/session/{session}/command"), &body)?;
    let texts: Vec<&str> = reply
        .get("parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_object())
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect();
    Ok(texts.join("\n"))
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    use serde_json::json;

    use super::super::clients::ServerAccess;
    use super::super::daemon;
    use super::*;

    struct Fake {
        _server: thread::JoinHandle<()>,
        base: String,
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            daemon::set_test_server(None);
        }
    }

    /// Answers each control endpoint once per request, by path suffix.
    fn answer_for(path: &str) -> (&'static str, Value) {
        if path.ends_with("/abort") {
            ("200 OK", json!(true))
        } else if path.ends_with("/fork") {
            ("200 OK", json!({"id": "ses_child"}))
        } else if path.ends_with("/share") {
            (
                "200 OK",
                json!({"id": "ses_1", "share": {"url": "https://s/1"}}),
            )
        } else if path.ends_with("/diff") {
            (
                "200 OK",
                json!([{"file": "src/a.rs", "before": "x", "after": "y", "additions": 3, "deletions": 1}]),
            )
        } else if path.ends_with("/agent") {
            (
                "200 OK",
                json!([{"name": "build", "description": "writes code", "mode": "primary"}]),
            )
        } else if path.ends_with("/command") {
            (
                "200 OK",
                json!({"parts": [
                    {"type": "text", "text": "done"},
                    {"type": "text", "text": "well"},
                    {"type": "reasoning", "text": "hidden"}
                ]}),
            )
        } else if path.ends_with("/config/providers") {
            (
                "200 OK",
                json!({"providers": [{
                    "id": "opencode-go",
                    "options": {
                        "apiKey": "test-key",
                        "headers": {"x-opencode-org-id": "wrk_test"}
                    }
                }]}),
            )
        } else if path.ends_with("/zen/go/v1/usage") {
            (
                "200 OK",
                json!({"usage": {
                    "rolling": {"status": "ok", "percent": 29, "resetsAt": "2026-10-12T23:53:31Z"},
                    "weekly": {"status": "ok", "percent": 54, "resetsAt": "2026-10-19T00:00:00Z"},
                    "monthly": {"status": "ok", "percent": 69, "resetsAt": "2026-11-10T17:05:00Z"}
                }}),
            )
        } else {
            ("404 Not Found", json!({"error": "unknown"}))
        }
    }

    fn fake() -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let mut stream = stream;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .split('?')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let (mut length, mut authorization) = (0usize, None::<String>);
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 {
                        break;
                    }
                    let header = header.trim();
                    if header.is_empty() {
                        break;
                    }
                    let lower = header.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    } else if lower.starts_with("authorization:") {
                        authorization = Some(header["authorization:".len()..].trim().to_string());
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let answer = if authorization.is_none() {
                    ("401 Unauthorized", json!({"error": "unauthorized"}))
                } else {
                    answer_for(&path)
                };
                let payload = answer.1.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    answer.0,
                    payload.len(),
                    payload
                );
                let _ = stream.flush();
            }
        });
        daemon::set_test_server(Some(ServerAccess {
            base: base.clone(),
            password: "pw".into(),
        }));
        Fake {
            _server: server,
            base,
        }
    }

    fn workdir() -> PathBuf {
        PathBuf::from("/work")
    }

    #[test]
    fn abort_fork_share_and_diff_hit_their_endpoints_with_auth() {
        let _fake = fake();
        let workdir = workdir();

        abort(&workdir, "ses_1").unwrap();
        assert_eq!(fork(&workdir, "ses_1").unwrap(), "ses_child");
        assert_eq!(share(&workdir, "ses_1").unwrap(), "https://s/1");
        let diffs = diff(&workdir, "ses_1").unwrap();
        assert_eq!(diffs[0]["file"], "src/a.rs");
        assert_eq!(diffs[0]["additions"], 3);
    }

    #[test]
    fn zen_usage_borrows_the_harness_credentials_and_parses_the_windows() {
        let fake = fake();
        let client = client_for(&workdir()).unwrap();
        let url = format!("{}/zen/go/v1/usage", fake.base);
        let plan = zen_usage_from(&client, "ses_1", &url).unwrap();
        assert_eq!(plan.five_hour_percent, Some(29.0));
        assert_eq!(
            plan.five_hour_resets_at.as_deref(),
            Some("2026-10-12T23:53:31Z")
        );
        assert_eq!(plan.week_percent, Some(54.0));
        assert_eq!(plan.week_resets_at.as_deref(), Some("2026-10-19T00:00:00Z"));
        assert_eq!(plan.month_percent, Some(69.0));
        assert_eq!(
            plan.month_resets_at.as_deref(),
            Some("2026-11-10T17:05:00Z")
        );
    }

    #[test]
    fn zen_usage_yields_none_when_the_account_answers_nothing() {
        let fake = fake();
        let client = client_for(&workdir()).unwrap();
        let url = format!("{}/nope", fake.base);
        assert!(zen_usage_from(&client, "ses_1", &url).is_none());
    }

    #[test]
    fn agents_and_model_defaults_list_the_server() {
        let _fake = fake();
        let workdir = workdir();

        let agents = agents(&workdir).unwrap();
        assert_eq!(agents[0]["name"], "build");
        assert_eq!(agents[0]["description"], "writes code");

        let defaults = model_defaults(&workdir).unwrap();
        assert_eq!(defaults["providers"], json!(["opencode-go"]));
        assert!(defaults["default"].is_null());
    }

    #[test]
    fn run_command_joins_text_parts_only() {
        let _fake = fake();
        let text = run_command(
            &workdir(),
            "ses_1",
            "init",
            "--force",
            Some("build"),
            Some("opencode/glm-5.3"),
        )
        .unwrap();
        assert_eq!(text, "done\nwell");
    }

    #[test]
    fn an_unreachable_server_is_an_error_not_a_panic() {
        daemon::set_test_server(Some(ServerAccess {
            base: "http://127.0.0.1:1".into(),
            password: "pw".into(),
        }));
        assert!(fork(&workdir(), "ses_1").is_err());
        daemon::set_test_server(None);
    }
}
