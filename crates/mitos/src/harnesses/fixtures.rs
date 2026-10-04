use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use crate::wire::events::AdapterEvent;

/// Held for a whole test: a fork elsewhere would inherit this script's write
/// handle and make its exec fail with ETXTBSY.
static SCRIPTS: Mutex<()> = Mutex::new(());

pub struct FakeProgram {
    _guard: MutexGuard<'static, ()>,
    pub dir: PathBuf,
    pub path: PathBuf,
}

impl FakeProgram {
    pub fn new(body: &str) -> Self {
        let guard = SCRIPTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("mitos-fake-{}", crate::domain::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _guard: guard,
            dir,
            path,
        }
    }
}

impl Drop for FakeProgram {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn kinds_of(events: &[AdapterEvent]) -> Vec<&str> {
    events.iter().map(|event| event.event.as_str()).collect()
}
