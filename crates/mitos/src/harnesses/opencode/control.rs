use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::clients::ServerClient;
use super::daemon;
use crate::json::string_value;

pub(crate) trait Control {
    fn abort(&self, workdir: &Path, session: &str) -> Result<()>;
    fn fork(&self, workdir: &Path, session: &str) -> Result<String>;
    fn share(&self, workdir: &Path, session: &str) -> Result<String>;
    fn diff(&self, workdir: &Path, session: &str) -> Result<Value>;
    fn agents(&self, workdir: &Path) -> Result<Value>;
    fn model_defaults(&self, workdir: &Path) -> Result<Value>;
    fn run_command(
        &self,
        workdir: &Path,
        session: &str,
        command: &str,
        arguments: &str,
        agent: Option<&str>,
        model: Option<&str>,
    ) -> Result<String>;
}

pub(crate) struct ServerControl;

impl Control for ServerControl {
    fn abort(&self, workdir: &Path, session: &str) -> Result<()> {
        abort(workdir, session)
    }

    fn fork(&self, workdir: &Path, session: &str) -> Result<String> {
        fork(workdir, session)
    }

    fn share(&self, workdir: &Path, session: &str) -> Result<String> {
        share(workdir, session)
    }

    fn diff(&self, workdir: &Path, session: &str) -> Result<Value> {
        diff(workdir, session)
    }

    fn agents(&self, workdir: &Path) -> Result<Value> {
        agents(workdir)
    }

    fn model_defaults(&self, workdir: &Path) -> Result<Value> {
        model_defaults(workdir)
    }

    fn run_command(
        &self,
        workdir: &Path,
        session: &str,
        command: &str,
        arguments: &str,
        agent: Option<&str>,
        model: Option<&str>,
    ) -> Result<String> {
        run_command(workdir, session, command, arguments, agent, model)
    }
}

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
    abort_with(&client, session)
}

fn abort_with(client: &ServerClient, session: &str) -> Result<()> {
    client.post(&format!("/session/{session}/abort"), &json!({}))?;
    Ok(())
}

/// Forks the session at its newest message; returns the child session id.
pub(crate) fn fork(workdir: &Path, session: &str) -> Result<String> {
    let client = client_for(workdir)?;
    fork_with(&client, session)
}

fn fork_with(client: &ServerClient, session: &str) -> Result<String> {
    let forked = client.post(&format!("/session/{session}/fork"), &json!({}))?;
    string_value(forked.get("id"))
        .map(str::to_string)
        .context("opencode did not return a forked session id")
}

/// Publishes the session; returns its share URL.
pub(crate) fn share(workdir: &Path, session: &str) -> Result<String> {
    let client = client_for(workdir)?;
    share_with(&client, session)
}

fn share_with(client: &ServerClient, session: &str) -> Result<String> {
    let shared = client.post(&format!("/session/{session}/share"), &json!({}))?;
    string_value(shared.pointer("/share/url"))
        .map(str::to_string)
        .context("opencode did not return a share url")
}

/// The session's file diffs: `{ file, before, after, additions, deletions }`.
pub(crate) fn diff(workdir: &Path, session: &str) -> Result<Value> {
    let client = client_for(workdir)?;
    diff_with(&client, session)
}

fn diff_with(client: &ServerClient, session: &str) -> Result<Value> {
    client.get(&format!("/session/{session}/diff"))
}

/// The server's agents: `{ name, description, mode, builtIn, ... }`.
pub(crate) fn agents(workdir: &Path) -> Result<Value> {
    let client = client_for(workdir)?;
    agents_with(&client)
}

fn agents_with(client: &ServerClient) -> Result<Value> {
    client.get("/agent")
}

/// The server's default model per provider, plus the provider ids.
pub(crate) fn model_defaults(workdir: &Path) -> Result<Value> {
    let client = client_for(workdir)?;
    model_defaults_with(&client)
}

fn model_defaults_with(client: &ServerClient) -> Result<Value> {
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
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .build(),
    );
    zen_usage_from_with_agent(client, session, url, &agent)
}

fn zen_usage_from_with_agent(
    client: &ServerClient,
    session: &str,
    url: &str,
    agent: &ureq::Agent,
) -> Option<ZenPlan> {
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
    run_command_with(&client, session, command, arguments, agent, model)
}

fn run_command_with(
    client: &ServerClient,
    session: &str,
    command: &str,
    arguments: &str,
    agent: Option<&str>,
    model: Option<&str>,
) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::super::clients::test_support::{
        agent_with_response, client_with_connection_error, client_with_response,
    };
    use super::super::clients::{ServerAccess, set_test_agent};
    use super::*;

    const PROVIDERS: &str = r#"{"providers":[{"id":"opencode-go","options":{"apiKey":"test-key","headers":{"x-opencode-org-id":"wrk_test"}}}]}"#;

    fn request_text(request: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(request.lock().unwrap().clone()).unwrap()
    }

    fn assert_get(request: &Arc<Mutex<Vec<u8>>>, path: &str) {
        let request = request_text(request);
        assert!(request.starts_with(&format!(
            "GET {path}?directory=%2Fwork%2Fmitos HTTP/1.1\r\n"
        )));
        assert!(request.contains("\r\nauthorization: Basic bWl0b3M6c2VjcmV0\r\n"));
    }

    fn assert_post(request: &Arc<Mutex<Vec<u8>>>, path: &str, body: &str) {
        let request = request_text(request);
        assert!(request.starts_with(&format!(
            "POST {path}?directory=%2Fwork%2Fmitos HTTP/1.1\r\n"
        )));
        assert!(request.contains("\r\nauthorization: Basic bWl0b3M6c2VjcmV0\r\n"));
        assert!(request.ends_with(&format!("\r\n\r\n{body}")));
    }

    struct TestServer;

    impl TestServer {
        fn install(status: &str, body: &str) -> (Self, Arc<Mutex<Vec<u8>>>) {
            let (agent, request) = agent_with_response(status, body);
            daemon::set_test_server(Some(ServerAccess {
                base: "http://opencode.test".into(),
                password: "secret".into(),
            }));
            set_test_agent(Some(agent));
            (Self, request)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            daemon::set_test_server(None);
            set_test_agent(None);
        }
    }

    #[test]
    fn abort_fork_share_and_diff_hit_their_endpoints_with_auth() {
        let (client, request) = client_with_response("200 OK", "true");
        abort_with(&client, "ses_1").unwrap();
        assert_post(&request, "/session/ses_1/abort", "{}");

        let (client, request) = client_with_response("200 OK", r#"{"id":"ses_child"}"#);
        assert_eq!(fork_with(&client, "ses_1").unwrap(), "ses_child");
        assert_post(&request, "/session/ses_1/fork", "{}");

        let (client, request) =
            client_with_response("200 OK", r#"{"id":"ses_1","share":{"url":"https://s/1"}}"#);
        assert_eq!(share_with(&client, "ses_1").unwrap(), "https://s/1");
        assert_post(&request, "/session/ses_1/share", "{}");

        let (client, request) = client_with_response(
            "200 OK",
            r#"[{"file":"src/a.rs","before":"x","after":"y","additions":3,"deletions":1}]"#,
        );
        let diffs = diff_with(&client, "ses_1").unwrap();
        assert_eq!(diffs[0]["file"], "src/a.rs");
        assert_eq!(diffs[0]["additions"], 3);
        assert_get(&request, "/session/ses_1/diff");
    }

    #[test]
    fn fork_and_share_reject_incomplete_responses() {
        let (client, _) = client_with_response("200 OK", "{}");
        assert_eq!(
            fork_with(&client, "ses_1").unwrap_err().to_string(),
            "opencode did not return a forked session id"
        );

        let (client, _) = client_with_response("200 OK", r#"{"share":{}}"#);
        assert_eq!(
            share_with(&client, "ses_1").unwrap_err().to_string(),
            "opencode did not return a share url"
        );
    }

    #[test]
    fn zen_usage_borrows_the_harness_credentials_and_parses_the_windows() {
        let (client, config_request) = client_with_response("200 OK", PROVIDERS);
        let (agent, usage_request) = agent_with_response(
            "200 OK",
            r#"{"usage":{"rolling":{"status":"ok","percent":29,"resetsAt":"2026-10-12T23:53:31Z"},"weekly":{"status":"ok","percent":54,"resetsAt":"2026-10-19T00:00:00Z"},"monthly":{"status":"ok","percent":69,"resetsAt":"2026-11-10T17:05:00Z"}}}"#,
        );
        let plan = zen_usage_from_with_agent(
            &client,
            "ses_1",
            "http://usage.test/zen/go/v1/usage",
            &agent,
        )
        .unwrap();
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
        assert_get(&config_request, "/config/providers");
        let usage_request = request_text(&usage_request);
        assert!(usage_request.starts_with("GET /zen/go/v1/usage HTTP/1.1\r\n"));
        assert!(usage_request.contains("\r\nauthorization: Bearer test-key\r\n"));
        assert!(usage_request.contains("\r\nuser-agent: mitos\r\n"));
        assert!(usage_request.contains("\r\nx-opencode-session: ses_1\r\n"));
        assert!(usage_request.contains("\r\nx-opencode-org-id: wrk_test\r\n"));
    }

    #[test]
    fn zen_usage_yields_none_when_the_account_answers_nothing() {
        let (client, _) = client_with_response("200 OK", PROVIDERS);
        let (agent, _) = agent_with_response("404 Not Found", r#"{"error":"unknown"}"#);
        assert!(
            zen_usage_from_with_agent(&client, "ses_1", "http://usage.test/nope", &agent).is_none()
        );
    }

    #[test]
    fn agents_and_model_defaults_list_the_server() {
        let (client, request) = client_with_response(
            "200 OK",
            r#"[{"name":"build","description":"writes code","mode":"primary"}]"#,
        );
        let agents = agents_with(&client).unwrap();
        assert_eq!(agents[0]["name"], "build");
        assert_eq!(agents[0]["description"], "writes code");
        assert_get(&request, "/agent");

        let (client, request) = client_with_response("200 OK", PROVIDERS);
        let defaults = model_defaults_with(&client).unwrap();
        assert_eq!(defaults["providers"], json!(["opencode-go"]));
        assert!(defaults["default"].is_null());
        assert_get(&request, "/config/providers");
    }

    #[test]
    fn model_defaults_keep_the_default_and_only_named_providers() {
        let (client, _) = client_with_response(
            "200 OK",
            r#"{"default":{"provider":"opencode","model":"glm"},"providers":[{"id":"opencode"},{"name":"missing id"},null]}"#,
        );

        assert_eq!(
            model_defaults_with(&client).unwrap(),
            json!({
                "default": { "provider": "opencode", "model": "glm" },
                "providers": ["opencode"],
            })
        );
    }

    #[test]
    fn run_command_joins_text_parts_only() {
        let (client, request) = client_with_response(
            "200 OK",
            r#"{"parts":[{"type":"text","text":"done"},{"type":"text","text":"well"},{"type":"reasoning","text":"hidden"}]}"#,
        );
        let text = run_command_with(
            &client,
            "ses_1",
            "init",
            "--force",
            Some("build"),
            Some("opencode/glm-5.3"),
        )
        .unwrap();
        assert_eq!(text, "done\nwell");
        assert_post(
            &request,
            "/session/ses_1/command",
            r#"{"command":"init","arguments":"--force","agent":"build","model":"opencode/glm-5.3"}"#,
        );
    }

    #[test]
    fn run_command_omits_optional_overrides_and_ignores_malformed_parts() {
        let (client, request) = client_with_response(
            "200 OK",
            r#"{"parts":[null,{"type":"text"},{"type":"tool","text":"hidden"},{"type":"text","text":"done"}]}"#,
        );

        assert_eq!(
            run_command_with(&client, "ses_1", "init", "", None, None).unwrap(),
            "done"
        );
        assert_post(
            &request,
            "/session/ses_1/command",
            r#"{"command":"init","arguments":""}"#,
        );
    }

    #[test]
    fn an_unreachable_server_is_an_error_not_a_panic() {
        assert!(fork_with(&client_with_connection_error(), "ses_1").is_err());
    }

    #[test]
    fn server_control_posts_turn_operations() {
        let (server, request) = TestServer::install("200 OK", "true");
        Control::abort(&ServerControl, Path::new("/work/mitos"), "ses_1").unwrap();
        assert_post(&request, "/session/ses_1/abort", "{}");
        drop(server);

        let (server, request) = TestServer::install("200 OK", r#"{"id":"ses_child"}"#);
        assert_eq!(
            Control::fork(&ServerControl, Path::new("/work/mitos"), "ses_1").unwrap(),
            "ses_child"
        );
        assert_post(&request, "/session/ses_1/fork", "{}");
        drop(server);

        let (_server, request) = TestServer::install(
            "200 OK",
            r#"{"share":{"url":"https://share.test/ses_child"}}"#,
        );
        assert_eq!(
            Control::share(&ServerControl, Path::new("/work/mitos"), "ses_1").unwrap(),
            "https://share.test/ses_child"
        );
        assert_post(&request, "/session/ses_1/share", "{}");
    }

    #[test]
    fn server_control_gets_diffs_agents_and_model_defaults() {
        let (server, request) = TestServer::install(
            "200 OK",
            r#"[{"file":"src/a.rs","additions":3,"deletions":1}]"#,
        );
        assert_eq!(
            Control::diff(&ServerControl, Path::new("/work/mitos"), "ses_1").unwrap()[0]["file"],
            "src/a.rs"
        );
        assert_get(&request, "/session/ses_1/diff");
        drop(server);

        let (server, request) = TestServer::install(
            "200 OK",
            r#"[{"name":"build","description":"writes code"}]"#,
        );
        assert_eq!(
            Control::agents(&ServerControl, Path::new("/work/mitos")).unwrap()[0]["name"],
            "build"
        );
        assert_get(&request, "/agent");
        drop(server);

        let (_server, request) = TestServer::install(
            "200 OK",
            r#"{"default":{"opencode":"glm"},"providers":[{"id":"opencode"},{"name":"missing id"}]}"#,
        );
        assert_eq!(
            Control::model_defaults(&ServerControl, Path::new("/work/mitos")).unwrap(),
            json!({
                "default": { "opencode": "glm" },
                "providers": ["opencode"],
            })
        );
        assert_get(&request, "/config/providers");
    }

    #[test]
    fn server_control_runs_commands_with_overrides() {
        let (_server, request) = TestServer::install(
            "200 OK",
            r#"{"parts":[{"type":"text","text":"done"},{"type":"reasoning","text":"hidden"}]}"#,
        );
        assert_eq!(
            Control::run_command(
                &ServerControl,
                Path::new("/work/mitos"),
                "ses_1",
                "init",
                "--force",
                Some("build"),
                Some("opencode/glm-5.3"),
            )
            .unwrap(),
            "done"
        );
        assert_post(
            &request,
            "/session/ses_1/command",
            r#"{"command":"init","arguments":"--force","agent":"build","model":"opencode/glm-5.3"}"#,
        );
    }
}
