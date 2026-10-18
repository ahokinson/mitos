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
fn abort_turn(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("abort", &target)?;
    control.abort(&target.workdir, session)?;
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

fn fork_thread(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
    json: bool,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("fork", &target)?;
    let child = control.fork(&target.workdir, session)?;
    let thread = service.new_bound_thread(id, HarnessKind::OpenCode, &Value::String(child))?;
    emit(json, thread, |thread| {
        println!("Forked thread {} from {id}.", thread.id);
    })
}

fn share_thread(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("share", &target)?;
    let url = control.share(&target.workdir, session)?;
    println!("{url}");
    Ok(())
}

fn show_diff(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
    json: bool,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("diff", &target)?;
    let diffs = control.diff(&target.workdir, session)?;
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

fn list_agents(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
) -> Result<()> {
    let target = service.native_target(id)?;
    opencode_session("agents", &target)?;
    let agents = control.agents(&target.workdir)?;
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

fn list_models(
    service: &ThreadService<'_>,
    control: &impl control::Control,
    id: &ThreadId,
) -> Result<()> {
    let target = service.native_target(id)?;
    opencode_session("models", &target)?;
    let defaults = control.model_defaults(&target.workdir)?;
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
    control: &impl control::Control,
    id: &ThreadId,
    command: &str,
    arguments: &str,
) -> Result<()> {
    let target = service.native_target(id)?;
    let session = opencode_session("command", &target)?;
    let text = control.run_command(
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
    dispatch_with_control(service, adapter, command, &control::ServerControl)
}

fn dispatch_with_control(
    service: &ThreadService<'_>,
    adapter: &Adapters,
    command: ThreadCommand,
    control: &impl control::Control,
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
        ThreadCommand::Abort { id } => abort_turn(service, control, &id),
        ThreadCommand::Fork { id, json } => fork_thread(service, control, &id, json),
        ThreadCommand::Share { id } => share_thread(service, control, &id),
        ThreadCommand::Diff { id, json } => show_diff(service, control, &id, json),
        ThreadCommand::Agent { id, name } => set_thread_agent(service, &id, name),
        ThreadCommand::Model { id, reference } => set_thread_model(service, &id, reference),
        ThreadCommand::Agents { id } => list_agents(service, control, &id),
        ThreadCommand::Models { id } => list_models(service, control, &id),
        ThreadCommand::Command {
            id,
            command,
            arguments,
        } => run_command(service, control, &id, &command, &arguments.join(" ")),
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use serde_json::{Value, json};

    use super::*;
    use crate::domain::{HarnessKind, RequestKind, Workspace};
    use crate::handoff::DeterministicRenderer;
    use crate::store::{NewHarnessRequest, Store};

    struct FakeControl {
        defaults: Value,
        command_output: String,
        command_fails: bool,
    }

    fn fake_control() -> FakeControl {
        FakeControl {
            defaults: json!({
                "default": {},
                "providers": [{"id": "opencode"}]
            }),
            command_output: "done".into(),
            command_fails: false,
        }
    }

    impl control::Control for FakeControl {
        fn abort(&self, _workdir: &Path, _session: &str) -> Result<()> {
            Ok(())
        }

        fn fork(&self, _workdir: &Path, _session: &str) -> Result<String> {
            Ok("ses_child".into())
        }

        fn share(&self, _workdir: &Path, _session: &str) -> Result<String> {
            Ok("https://s/1".into())
        }

        fn diff(&self, _workdir: &Path, _session: &str) -> Result<Value> {
            Ok(json!([{
                "file": "src/a.rs",
                "before": "x",
                "after": "y",
                "additions": 3,
                "deletions": 1
            }]))
        }

        fn agents(&self, _workdir: &Path) -> Result<Value> {
            Ok(json!([{
                "name": "build",
                "description": "writes code",
                "mode": "primary"
            }]))
        }

        fn model_defaults(&self, _workdir: &Path) -> Result<Value> {
            Ok(self.defaults.clone())
        }

        fn run_command(
            &self,
            _workdir: &Path,
            _session: &str,
            _command: &str,
            _arguments: &str,
            _agent: Option<&str>,
            _model: Option<&str>,
        ) -> Result<String> {
            if self.command_fails {
                bail!("the command failed");
            }
            Ok(self.command_output.clone())
        }
    }

    struct Rig {
        root: PathBuf,
        workspace: Workspace,
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
        let store = Store::new(Some(root.clone())).unwrap();
        let workspace_root = root.join("workspace");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let workspace = store
            .workspace_for(&workspace_root, None, "workspace")
            .unwrap();
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
            let thread = self.store.create_thread(&self.workspace.id).unwrap();
            self.store
                .set_thread_harness(&thread.id, Some(harness), Some(&json!(session)))
                .unwrap();
            thread.id
        }

        fn thread_count(&self) -> usize {
            self.store.list_threads(&self.workspace.id).unwrap().len()
        }
    }

    #[test]
    fn opencode_control_commands_use_the_control_api() {
        let control = fake_control();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        assert!(abort_turn(&rig.service(), &control, &id).is_ok());
        assert!(fork_thread(&rig.service(), &control, &id, true).is_ok());
        assert!(share_thread(&rig.service(), &control, &id).is_ok());
        assert!(show_diff(&rig.service(), &control, &id, false).is_ok());
        assert!(show_diff(&rig.service(), &control, &id, true).is_ok());
        assert!(list_agents(&rig.service(), &control, &id).is_ok());
        assert!(list_models(&rig.service(), &control, &id).is_ok());
        assert!(run_command(&rig.service(), &control, &id, "init", "--force").is_ok());
    }

    #[test]
    fn only_opencode_threads_take_control_commands() {
        let control = fake_control();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::Claude, "s-1");

        let refused = fork_thread(&rig.service(), &control, &id, false).unwrap_err();
        assert!(refused.to_string().contains("only OpenCode threads"));
    }

    #[test]
    fn forking_binds_the_child_session_as_a_new_thread() {
        let control = fake_control();
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");
        assert_eq!(rig.thread_count(), 1);

        fork_thread(&rig.service(), &control, &id, false).unwrap();
        assert_eq!(rig.thread_count(), 2);
    }

    #[test]
    fn sync_and_requests_print_both_shapes() {
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        assert!(sync(&rig.service(), &id, 0, true).is_ok());
        assert!(sync(&rig.service(), &id, 0, false).is_ok());
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

    #[test]
    fn reply_flags_produce_denials_and_reject_empty_replies() {
        assert_eq!(
            reply_from(false, true, Some("not now".into())).unwrap(),
            Reply::Deny(Some("not now".into()))
        );
        assert!(reply_from(false, false, None).is_err());
    }

    #[test]
    fn dispatches_thread_creation_listing_and_pending_requests() {
        let rig = rig();
        let default_control = fake_control();
        let workspace = rig.root.join("new-workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::New {
                harness: Some(HarnessKind::Claude),
                workspace: workspace.clone(),
                json: false,
            },
            &default_control,
        )
        .unwrap();
        dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::List {
                workspace,
                json: false,
            },
            &default_control,
        )
        .unwrap();

        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");
        let request = rig
            .store
            .open_request(NewHarnessRequest {
                thread_id: id.clone(),
                turn_id: None,
                harness: Some(HarnessKind::OpenCode),
                kind: RequestKind::Question,
                payload: Some(json!({ "id": "ask_1" })),
            })
            .unwrap();
        dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::Requests {
                id: id.clone(),
                json: false,
            },
            &default_control,
        )
        .unwrap();
        assert_eq!(request.thread_id, id);
    }

    #[test]
    fn dispatches_opencode_control_commands() {
        let rig = rig();
        let default_control = fake_control();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");

        for command in [
            ThreadCommand::Abort { id: id.clone() },
            ThreadCommand::Fork {
                id: id.clone(),
                json: false,
            },
            ThreadCommand::Share { id: id.clone() },
            ThreadCommand::Diff {
                id: id.clone(),
                json: false,
            },
            ThreadCommand::Diff {
                id: id.clone(),
                json: true,
            },
            ThreadCommand::Agents { id: id.clone() },
        ] {
            dispatch_with_control(&rig.service(), &Adapters, command, &default_control).unwrap();
        }

        let configured_control = FakeControl {
            defaults: json!({ "default": { "opencode": "gpt-5" } }),
            command_output: String::new(),
            command_fails: false,
        };
        dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::Models { id: id.clone() },
            &configured_control,
        )
        .unwrap();
        dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::Command {
                id: id.clone(),
                command: "init".into(),
                arguments: vec!["--force".into()],
            },
            &configured_control,
        )
        .unwrap();

        let failing_control = FakeControl {
            command_fails: true,
            ..fake_control()
        };
        let error = dispatch_with_control(
            &rig.service(),
            &Adapters,
            ThreadCommand::Command {
                id: id.clone(),
                command: "init".into(),
                arguments: Vec::new(),
            },
            &failing_control,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "the command failed");
    }

    #[test]
    fn answers_pending_requests_without_a_socket() {
        let rig = rig();
        let id = rig.bound_thread(HarnessKind::OpenCode, "ses_1");
        let question = rig
            .store
            .open_request(NewHarnessRequest {
                thread_id: id.clone(),
                turn_id: None,
                harness: Some(HarnessKind::OpenCode),
                kind: RequestKind::Question,
                payload: Some(json!({ "id": "ask_1" })),
            })
            .unwrap();
        let permission = rig
            .store
            .open_request(NewHarnessRequest {
                thread_id: id.clone(),
                turn_id: None,
                harness: Some(HarnessKind::OpenCode),
                kind: RequestKind::Permission,
                payload: Some(json!({ "id": "allow_1" })),
            })
            .unwrap();
        let raw = rig
            .store
            .open_request(NewHarnessRequest {
                thread_id: id.clone(),
                turn_id: None,
                harness: Some(HarnessKind::OpenCode),
                kind: RequestKind::Permission,
                payload: Some(json!({ "id": "raw_1" })),
            })
            .unwrap();
        let delivered = RefCell::new(Vec::new());
        let deliver = |_: &Path, line: &Value| {
            delivered.borrow_mut().push(line.clone());
            Ok(())
        };
        let service = ThreadService::with_answer_delivery(&rig.store, &rig.renderer, &deliver);

        answer_request(
            &service,
            &id,
            &question.id,
            false,
            false,
            Some("blue".into()),
            None,
        )
        .unwrap();
        answer_request(&service, &id, &permission.id, true, false, None, None).unwrap();
        answer_request(
            &service,
            &id,
            &raw.id,
            false,
            false,
            None,
            Some("plain response".into()),
        )
        .unwrap();

        assert_eq!(
            *delivered.borrow(),
            [
                json!({ "action": "answer", "request_id": "ask_1", "response": { "answer": "blue" } }),
                json!({ "action": "answer", "request_id": "allow_1", "response": { "allow": true } }),
                json!({ "action": "answer", "request_id": "raw_1", "response": "plain response" }),
            ]
        );
        assert!(service.pending_requests(&id).unwrap().is_empty());
    }
}
