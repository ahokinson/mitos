use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::args::{EnterArgs, ThreadCommand};
use super::outputs::emit;
use crate::domain::{HarnessKind, Reply, ThreadId};
use crate::harnesses::Adapters;
use crate::ports::HarnessRunner;
use crate::runner::ptys::PtyRunner;
use crate::service::{EventView, ThreadService};

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
        ThreadCommand::Answer {
            id,
            request,
            approve,
            deny,
            text,
            response,
        } => {
            match response {
                Some(raw) => {
                    let response = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
                    service.answer(&id, &request, &response)?;
                }
                None => {
                    service.reply(&id, &request, &reply_from(approve, deny, text)?)?;
                }
            }
            println!("Answered request {request}.");
            Ok(())
        }
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
