use std::fs::{self, OpenOptions};
use std::path::Path;

use anyhow::{Context, Result};
#[cfg(unix)]
use nix::fcntl::{Flock, FlockArg};

pub struct StoreLock {
    #[cfg(unix)]
    _file: Flock<fs::File>,
    #[cfg(windows)]
    _file: fs::File,
}

#[cfg(unix)]
pub(super) fn lock_file(path: &Path) -> Result<StoreLock> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("cannot open lock file {}", path.display()))?;
    let file = Flock::lock(file, FlockArg::LockExclusive)
        .map_err(|(_, error)| error)
        .with_context(|| format!("cannot lock {}", path.display()))?;
    Ok(StoreLock { _file: file })
}

#[cfg(windows)]
pub(super) fn lock_file(path: &Path) -> Result<StoreLock> {
    use std::os::windows::io::AsRawHandle;

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("cannot open lock file {}", path.display()))?;
    let mut overlapped = Overlapped::default();
    let locked = unsafe {
        lock_file_ex(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if locked == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("cannot lock {}", path.display()));
    }
    Ok(StoreLock { _file: file })
}

#[cfg(windows)]
const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: *mut std::ffi::c_void,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "LockFileEx"]
    fn lock_file_ex(
        file: *mut std::ffi::c_void,
        flags: u32,
        reserved: u32,
        bytes_low: u32,
        bytes_high: u32,
        overlapped: *mut Overlapped,
    ) -> i32;
}
