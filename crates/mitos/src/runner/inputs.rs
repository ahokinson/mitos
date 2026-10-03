#[cfg(unix)]
use std::io::Read;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(unix))]
use std::time::Duration;

#[cfg(not(unix))]
use crossterm::{
    event,
    event::{Event, KeyCode, KeyModifiers},
};
#[cfg(unix)]
use nix::poll::{PollFd, PollFlags, poll};

#[cfg(unix)]
pub fn forward_input(mut writer: Box<dyn Write + Send>, forwarding: &AtomicBool) {
    use std::os::fd::AsFd;

    let stdin = io::stdin();
    let mut buffer = [0; 4096];
    while forwarding.load(Ordering::Acquire) {
        let mut descriptors = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        if poll(&mut descriptors, 100_u16).is_err() {
            break;
        }
        let ready = descriptors[0]
            .revents()
            .is_some_and(|events| events.intersects(PollFlags::POLLIN | PollFlags::POLLHUP));
        if !ready {
            continue;
        }
        let Ok(read) = stdin.lock().read(&mut buffer) else {
            break;
        };
        if read == 0 || writer.write_all(&buffer[..read]).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

#[cfg(not(unix))]
pub fn forward_input(mut writer: Box<dyn Write + Send>, forwarding: &AtomicBool) {
    while forwarding.load(Ordering::Acquire) {
        let Ok(ready) = event::poll(Duration::from_millis(100)) else {
            break;
        };
        if !ready {
            continue;
        }
        let Ok(event) = event::read() else {
            break;
        };
        if write_event(&mut writer, event).is_err() {
            break;
        }
    }
}

#[cfg(not(unix))]
fn write_event(writer: &mut dyn Write, event: Event) -> io::Result<()> {
    let Event::Key(key) = event else {
        return Ok(());
    };
    if !key.is_press() && !key.is_repeat() {
        return Ok(());
    }
    let mut bytes = if let KeyCode::Char(character) = key.code {
        if key.modifiers.contains(KeyModifiers::CONTROL) && character.is_ascii() {
            vec![(character.to_ascii_lowercase() as u8) & 0x1f]
        } else {
            character.to_string().into_bytes()
        }
    } else {
        match key.code {
            KeyCode::Backspace => vec![0x7f],
            KeyCode::Enter => vec![b'\r'],
            KeyCode::Tab => vec![b'\t'],
            KeyCode::BackTab => b"\x1b[Z".to_vec(),
            KeyCode::Esc => vec![0x1b],
            KeyCode::Up => b"\x1b[A".to_vec(),
            KeyCode::Down => b"\x1b[B".to_vec(),
            KeyCode::Right => b"\x1b[C".to_vec(),
            KeyCode::Left => b"\x1b[D".to_vec(),
            KeyCode::Home => b"\x1b[H".to_vec(),
            KeyCode::End => b"\x1b[F".to_vec(),
            KeyCode::PageUp => b"\x1b[5~".to_vec(),
            KeyCode::PageDown => b"\x1b[6~".to_vec(),
            KeyCode::Delete => b"\x1b[3~".to_vec(),
            KeyCode::Insert => b"\x1b[2~".to_vec(),
            KeyCode::F(number) => format!("\x1b[{number}~").into_bytes(),
            _ => return Ok(()),
        }
    };
    if key.modifiers.contains(KeyModifiers::ALT) {
        bytes.insert(0, 0x1b);
    }
    writer.write_all(&bytes)?;
    writer.flush()
}
