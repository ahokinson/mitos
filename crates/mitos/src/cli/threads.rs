use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::args::{EnterArgs, ThreadCommand};
use super::outputs::emit;
use crate::domain::{HarnessKind, Reply, RequestId, ThreadId};
use crate::harnesses::Adapters;
use crate::harnesses::opencode::control;
use crate::ports::HarnessRunner;
use crate::runner::ptys::PtyRunner;
use crate::service::{EventView, NativeTarget, ThreadService};

fn opencode_session<'a>(operation: &str, target: &'a NativeTarget) -> Result<&'a str> {
    if target.harness != HarnessKind::OpenCode {
        bail!(
            "only OpenCode threads support {operation}; this thread is on {}",
            target.harness
        );
    }
    Ok(&target.session)
}
fn abort_turn(service: &ThreadService<'_>, id: &ThreadId) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("abort", &target)?;
    control::abort(&target.workdir, session)?;
    println!("Asked OpenCode to stop the running turn on thread {id}.");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn answer_request(
    service: &ThreadService<'_>,
    id: &ThreadId,
    request: &RequestId,
    approve: bool,
    deny: bool,
    text: Option<String>,
    response: Option<String>,
) -> Result<()> {
    match response {
        Some(raw) => {
            let response = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            service.answer(id, request, &response)?;
        }
        None => {
            service.reply(id, request, &reply_from(approve, deny, text)?)?;
        }
    }
    println!("Answered request {request}.");
    Ok(())
}

fn set_thread_agent(
    service: &ThreadService<'_>,
    id: &ThreadId,
    name: Option<String>,
) -> Result<()> {
    service.set_agent(id, name.as_deref())?;
    match name {
        Some(name) => println!("Thread {id} uses the {name} agent."),
        None => println!("Thread {id} follows its mode's default agent."),
    }
    Ok(())
}

fn set_thread_model(
    service: &ThreadService<'_>,
    id: &ThreadId,
    reference: Option<String>,
) -> Result<()> {
    service.set_model(id, reference.as_deref())?;
    match reference {
        Some(reference) => println!("Thread {id} uses {reference}."),
        None => println!("Thread {id} follows its harness's default model."),
    }
    Ok(())
}

fn fork_thread(service: &ThreadService<'_>, id: &ThreadId, json: bool) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("fork", &target)?;
    let child = control::fork(&target.workdir, session)?;
    let thread = service.new_bound_thread(id, HarnessKind::OpenCode, &Value::String(child))?;
    emit(json, thread, |thread| {
        println!("Forked thread {} from {id}.", thread.id);
    })
}

fn share_thread(service: &ThreadService<'_>, id: &ThreadId) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("share", &target)?;
    let url = control::share(&target.workdir, session)?;
    println!("{url}");
    Ok(())
}

fn show_diff(service: &ThreadService<'_>, id: &ThreadId, json: bool) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("diff", &target)?;
    let diffs = control::diff(&target.workdir, session)?;
    if json {
        println!("{}", serde_json::to_string(&diffs)?);
    } else {
        for diff in diffs.as_array().into_iter().flatten() {
            let file = diff.get("file").and_then(Value::as_str).unwrap_or("?");
            let added = diff.get("additions").and_then(Value::as_u64).unwrap_or(0);
            let removed = diff.get("deletions").and_then(Value::as_u64).unwrap_or(0);
            println!("{file}: +{added} -{removed}");
        }
    }
    Ok(())
}

fn list_agents(service: &ThreadService<'_>, id: &ThreadId) -> Result<()> {
    let target = service.native_target(id)?;
    opencode_session("agents", &target)?;
    let agents = control::agents(&target.workdir)?;
    for agent in agents.as_array().into_iter().flatten() {
        let name = agent.get("name").and_then(Value::as_str).unwrap_or("?");
        let description = agent
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        println!("{name}: {description}");
    }
    Ok(())
}

fn list_models(service: &ThreadService<'_>, id: &ThreadId) -> Result<()> {
    let target = service.native_target(id)?;
    opencode_session("models", &target)?;
    let defaults = control::model_defaults(&target.workdir)?;
    let defaults = defaults
        .get("default")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if defaults.is_empty() {
        println!("The server reported no default models.");
    }
    for (provider, default) in &defaults {
        println!("{provider}/{default}");
    }
    Ok(())
}

fn run_command(
    service: &ThreadService<'_>,
    id: &ThreadId,
    command: &str,
    arguments: &str,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("command", &target)?;
    let text = control::run_command(
        &target.workdir,
        session,
        command,
        arguments,
        target.agent.as_deref(),
        target.model.as_deref(),
    )?;
    if text.is_empty() {
        println!("The command returned no text.");
    } else {
        println!("{text}");
    }
    Ok(())
}

pub fn dispatch(
    service: &ThreadService<'_>,
    adapter: &Adapters,
    command: ThreadCommand,
) -> Result<()> {
    match command {
        ThreadCommand::New {
            harness,
            workspace,
            json,
        } => {
            let thread = service.new_thread(harness, &workspace)?;
            emit(json, thread, |thread| {
                println!("Created thread {}", thread.id);
            })
        }
        ThreadCommand::List { workspace, json } => {
            let threads = service.list_threads(&workspace)?;
            emit(json, threads, |threads| {
                for thread in threads {
                    println!(
                        "{} [{}] {}",
                        thread.id,
                        thread
                            .active_harness
                            .map_or("unassigned", HarnessKind::as_str),
                        thread.status.as_str(),
                    );
                }
            })
        }
        ThreadCommand::Send { id, message } => {
            let turn_id = service.send(&id, message, adapter)?;
            println!("Queued turn {turn_id} on thread {id}.");
            Ok(())
        }
        ThreadCommand::Sync { id, since, json } => sync(service, &id, since, json),
        ThreadCommand::Reassign { id, to } => {
            service.reassign_harness(&id, to, adapter)?;
            println!("Reassigned thread {id} to {to}.");
            Ok(())
        }
        ThreadCommand::Compact { id, mode } => {
            service.compact_thread(&id, mode, adapter)?;
            println!("Compacted thread {id} ({}).", mode.as_str());
            Ok(())
        }
        ThreadCommand::Attach { id } => service.attach(&id, adapter),
        ThreadCommand::Archive { id } => {
            service.archive(&id, adapter)?;
            println!("Archived thread {id}.");
            Ok(())
        }
        ThreadCommand::Delete { id } => {
            service.delete(&id, adapter)?;
            println!("Deleted thread {id}.");
            Ok(())
        }
        ThreadCommand::Note { id, notes } => service.note(&id, notes.into()),
        ThreadCommand::Drive { id, turn } => service.drive(&id, &turn, adapter),
        ThreadCommand::Mode { id, mode } => {
            service.set_mode(&id, mode)?;
            println!("Thread {id} is in {} mode.", mode.as_str());
            Ok(())
        }
        ThreadCommand::Abort { id } => abort_turn(service, &id),
        ThreadCommand::Fork { id, json } => fork_thread(service, &id, json),
        ThreadCommand::Share { id } => share_thread(service, &id),
        ThreadCommand::Diff { id, json } => show_diff(service, &id, json),
        ThreadCommand::Agent { id, name } => set_thread_agent(service, &id, name),
        ThreadCommand::Model { id, reference } => set_thread_model(service, &id, reference),
        ThreadCommand::Agents { id } => list_agents(service, &id),
        ThreadCommand::Models { id } => list_models(service, &id),
        ThreadCommand::Command {
            id,
            command,
            arguments,
        } => run_command(service, &id, &command, &arguments.join(" ")),
        ThreadCommand::Answer {
            id,
            request,
            approve,
            deny,
            text,
            response,
        } => answer_request(service, &id, &request, approve, deny, text, response),
        ThreadCommand::Requests { id, json } => {
            let pending = service.pending_requests(&id)?;
            emit(json, pending, |pending| {
                for request in pending {
                    println!("{} [{}]", request.id, request.kind.as_str());
                }
            })
        }
    }
}

fn sync(service: &ThreadService<'_>, id: &ThreadId, since: i64, json: bool) -> Result<()> {
    let events = service.event_views(id, since)?;
    emit(json, events, |events| {
        for EventView { event, .. } in events {
            println!(
                "[{}] {}: {}",
                event.seq,
                event.kind.as_str(),
                event.content.unwrap_or_default()
            );
        }
    })
}

fn reply_from(approve: bool, deny: bool, text: Option<String>) -> Result<Reply> {
    match (approve, deny, text) {
        (true, _, _) => Ok(Reply::Approve),
        (_, true, note) => Ok(Reply::Deny(note)),
        (_, _, Some(text)) => Ok(Reply::Text(text)),
        _ => bail!("provide --approve, --deny, --text, or --response"),
    }
}

pub fn enter(service: &ThreadService<'_>, adapter: &Adapters, args: EnterArgs) -> Result<()> {
    let harness = args.harness;
    let entry = service.begin_entry(&args.thread, harness, args.native_session, adapter)?;
    println!(
        "\n--- Mitos handoff to {harness} ---\n{}\n--- entering native harness; exit it normally to continue ---\n",
        entry.context
    );

    let outcome = PtyRunner.run(&entry.launch, &entry.workdir);

    if let Some(error) = service.finish_entry(entry, args.notes.into(), adapter)? {
        eprintln!("Mitos could not mine the {harness} handoff: {error:#}");
    }
    let status =
        outcome.context("harness ended before Mitos could complete its terminal session")?;
    if status.success {
        println!("{harness} exited.");
    } else {
        println!("{harness} exited with {}.", status.description);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;

    use serde_json::json;

    use super::*;
    use crate::domain::HarnessKind;
    use crate::git::Checkout;
    use crate::handoff::DeterministicRenderer;
    use crate::harnesses::opencode::daemon::{self, ServerAccess};
    use crate::store::Store;

    /// Answers each control endpoint with fixed JSON once authorized, the
    /// same fake `opencode` server shape `control`'s own tests use.
    struct FakeServer {
        _server: thread::JoinHandle<()>,
    }

    impl Drop for FakeServer {
        fn drop(&mut self) {
            daemon::set_test_server(None);
        }
    }

    fn fake_server() -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                respond(stream);
            }
        });
        daemon::set_test_server(Some(ServerAccess {
            base,
            password: "pw".into(),
        }));
        FakeServer { _server: server }
    }

    fn respond(mut stream: TcpStream) {
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
        let (mut length, mut authorized) = (0usize, false);
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
            if lower.starts_with("authorization:") {
                authorized = true;
            } else if let Some(value) = lower.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        let _ = reader.read_exact(&mut body);
        let answer = if !authorized {
            ("401 Unauthorized", json!({"error": "unauthorized"}))
        } else if path.ends_with("/abort") {
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
                json!({"parts": [{"type": "text", "text": "done"}]}),
            )
        } else if path.ends_with("/config/providers") {
            (
                "200 OK",
                json!({"default": {"opencode": "glm-5.3"}, "providers": [{"id": "opencode"}]}),
            )
        } else {
            ("404 Not Found", json!({"error": "unknown"}))
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

    struct Rig {
        root: PathBuf,
        workspace: PathBuf,
        store: Store,
        renderer: DeterministicRenderer,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn rig() -> Rig {
        let root = std::env::temp_dir().join(format!("mitos-cli-threads-{}", crate::domain::id()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let store = Store::new(Some(root.clone())).unwrap();
        Rig {
            root,
            workspace,
            store,
            renderer: DeterministicRenderer,
        }
    }

    impl Rig {
        fn service(&self) -> ThreadService<'_> {
            ThreadService::new(&self.store, &self.renderer)
        }

        fn bound_thread(&self, harness: HarnessKind, session: &str) -> ThreadId {
            let checkout = Checkout::open(&self.workspace).unwrap();
            let workspace = self
                .store
                .workspace_for(checkout.root(), checkout.git_dir(), &checkout.key())
                .unwrap();
            let thread = self.store.create_thread(&workspace.id).unwrap();
            self.store
                .set_thread_harness(&thread.id, Some(harness), Some(&json!(session)))
                .unwrap();
            thread.id
        }

        fn thread_count(&self) -> usize {
            let checkout = Checkout::open(&self.workspace).unwrap();
            let workspace = self
                .store
                .workspace_for(checkout.root(), checkout.git_dir(), &checkout.key())
                .unwrap();
            self.store.list_threads(&workspace.id).unwrap().len()
        }
    }

    #[test]
    fn opencode_control_commands_run_against_the_shared_server() {
        let _fake = fake_server();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        assert!(abort_turn(&rig.service(), &id).is_ok());
        assert!(fork_thread(&rig.service(), &id, true).is_ok());
        assert!(share_thread(&rig.service(), &id).is_ok());
        assert!(show_diff(&rig.service(), &id, false).is_ok());
        assert!(list_agents(&rig.service(), &id).is_ok());
        assert!(list_models(&rig.service(), &id).is_ok());
        assert!(run_command(&rig.service(), &id, "init", "--force").is_ok());
    }

    #[test]
    fn only_opencode_threads_take_control_commands() {
        let _fake = fake_server();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::Claude, "s-1");

        let refused = fork_thread(&rig.service(), &id, false).unwrap_err();
        assert!(refused.to_string().contains("only OpenCode threads"));
    }

    #[test]
    fn forking_binds_the_child_session_as_a_new_thread() {
        let _fake = fake_server();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");
        assert_eq!(rig.thread_count(), 1);

        fork_thread(&rig.service(), &id, false).unwrap();
        assert_eq!(rig.thread_count(), 2);
    }

    #[test]
    fn sync_list_and_requests_print_both_shapes() {
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        assert!(sync(&rig.service(), &id, 0, true).is_ok());
        assert!(sync(&rig.service(), &id, 0, false).is_ok());
        assert!(
            dispatch(
                &rig.service(),
                &Adapters,
                ThreadCommand::List {
                    workspace: rig.workspace.clone(),
                    json: false,
                },
            )
            .is_ok()
        );
        assert!(
            dispatch(
                &rig.service(),
                &Adapters,
                ThreadCommand::Requests {
                    id: id.clone(),
                    json: false,
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn agent_and_model_setters_run_without_a_server() {
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        assert!(set_thread_agent(&rig.service(), &id, Some("build".into())).is_ok());
        assert!(set_thread_agent(&rig.service(), &id, None).is_ok());
        assert!(set_thread_model(&rig.service(), &id, Some("opencode/glm-5.3".into())).is_ok());
        assert!(set_thread_model(&rig.service(), &id, None).is_ok());
    }
}
