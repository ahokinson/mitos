use std::sync::Arc;
#[cfg(unix)]
use std::sync::Mutex;
use std::thread;

use anyhow::{Context, Result};
use crossterm::terminal;
#[cfg(unix)]
use portable_pty::MasterPty;
use portable_pty::PtySize;
#[cfg(unix)]
use signal_hook::consts::signal::SIGWINCH;
#[cfg(unix)]
use signal_hook::iterator::Signals;

pub fn current_size() -> PtySize {
    match terminal::size() {
        Ok((cols, rows)) => PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        Err(_) => PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    }
}

#[cfg(unix)]
pub fn forward_resize(
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
) -> Option<(signal_hook::iterator::Handle, thread::JoinHandle<()>)> {
    let Ok(mut signals) = Signals::new([SIGWINCH]) else {
        return None;
    };
    let control = signals.handle();
    let thread = thread::spawn(move || {
        for _ in signals.forever() {
            if let Ok(master) = master.lock() {
                let _ = master.resize(current_size());
            }
        }
    });
    Some((control, thread))
}

pub struct RawMode;
impl RawMode {
    pub fn enable() -> Result<Self> {
        terminal::enable_raw_mode().context("could not enable terminal raw mode")?;
        Ok(Self)
    }
}
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
