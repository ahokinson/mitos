use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
#[cfg(unix)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use anyhow::{Context, Result};
#[cfg(unix)]
use portable_pty::MasterPty;
use portable_pty::{CommandBuilder, native_pty_system};

use super::inputs::forward_input;
#[cfg(unix)]
use super::terminals::forward_resize;
use super::terminals::{RawMode, current_size};
use crate::ports::{HarnessRunner, RunStatus};
use crate::wire::responses::LaunchPlan;

pub struct PtyRunner;

impl HarnessRunner for PtyRunner {
    fn run(&self, launch: &LaunchPlan, workdir: &Path) -> Result<RunStatus> {
        run(launch, workdir)
    }
}

fn run(launch: &LaunchPlan, workdir: &Path) -> Result<RunStatus> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(current_size())
        .context("could not create a pseudoterminal")?;
    let mut command = CommandBuilder::new(&launch.program);
    command.args(&launch.args);
    command.cwd(workdir);
    for (key, value) in &launch.env {
        command.env(key, value);
    }
    command.env("MITOS_ACTIVE", "1");
    let mut reader = pair
        .master
        .try_clone_reader()
        .context("could not read harness pseudoterminal")?;
    let writer = pair
        .master
        .take_writer()
        .context("could not write harness pseudoterminal")?;
    #[cfg(unix)]
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(pair.master));
    #[cfg(not(unix))]
    let _master = pair.master;

    // Raw mode first, so a setup failure cannot leave a harness running uncontrolled.
    let raw = RawMode::enable()?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .context("could not launch harness in pseudoterminal")?;
    drop(pair.slave);

    #[cfg(unix)]
    let resize_forwarder = forward_resize(Arc::clone(&master));
    let forwarding = Arc::new(AtomicBool::new(true));

    let output = thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        let _ = io::copy(&mut reader, &mut stdout);
        let _ = stdout.flush();
    });
    let input_forwarding = Arc::clone(&forwarding);
    let input = thread::spawn(move || {
        forward_input(writer, &input_forwarding);
    });

    let status = child.wait().context("could not wait for harness");
    forwarding.store(false, Ordering::Release);
    let _ = input.join();
    #[cfg(unix)]
    if let Some((resize_control, resize_thread)) = resize_forwarder {
        resize_control.close();
        // Not joined: `Signals::forever` may not wake after close, and waiting would hang.
        let _ = resize_thread;
    }
    drop(raw);
    let _ = output.join();
    let status = status?;
    Ok(RunStatus {
        success: status.success(),
        description: status.to_string(),
    })
}
