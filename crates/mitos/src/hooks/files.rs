use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

const NIX_STORE: &str = "/nix/store";

/// Follows symlinks, so an edit lands on the real file and the link survives.
pub fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Why Mitos cannot write `path`, or `None` when it can. Probes without
/// modifying anything.
pub fn blocked_reason(path: &Path) -> Option<String> {
    let target = resolved(path);
    if target.starts_with(NIX_STORE) {
        return Some(format!("managed by Nix, read-only ({})", target.display()));
    }
    if target.exists() {
        return OpenOptions::new()
            .append(true)
            .open(&target)
            .err()
            .map(|error| format!("{} is not writable ({error})", target.display()));
    }
    let directory = target.ancestors().skip(1).find(|dir| dir.exists())?;
    fs::metadata(directory)
        .ok()
        .filter(|meta| meta.permissions().readonly())
        .map(|_| format!("{} is read-only", directory.display()))
}

/// `Ok(None)` when there is nothing to back up.
pub fn backup(path: &Path) -> io::Result<Option<PathBuf>> {
    let target = resolved(path);
    if !target.exists() {
        return Ok(None);
    }
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let destination = target.with_file_name(format!("{name}.mitos-backup-{stamp}"));
    fs::copy(&target, &destination)?;
    Ok(Some(destination))
}

/// Writes through a temp file in the same directory so a crash never leaves
/// a half-written config, keeping the original's permissions.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let target = resolved(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = target.with_file_name(format!("{name}.mitos-tmp"));
    fs::write(&temporary, contents)?;
    if let Ok(meta) = fs::metadata(&target) {
        fs::set_permissions(&temporary, meta.permissions())?;
    }
    fs::rename(&temporary, &target)
}

pub fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

#[cfg(test)]
pub fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mitos-hooks-test-{}", crate::domain::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::{backup, blocked_reason, resolved, scratch_dir, write_atomic};

    #[test]
    fn a_writable_file_and_a_missing_one_in_a_writable_directory_are_not_blocked() {
        let dir = scratch_dir();
        let file = dir.join("settings.json");
        assert_eq!(blocked_reason(&file), None);
        fs::write(&file, "{}").unwrap();
        assert_eq!(blocked_reason(&file), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_read_only_file_is_blocked_with_its_path() {
        let dir = scratch_dir();
        let file = dir.join("config.yaml");
        fs::write(&file, "a: 1").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();

        let reason = blocked_reason(&file).unwrap();

        assert!(reason.contains("not writable"), "{reason}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_in_a_read_only_directory_is_blocked() {
        let dir = scratch_dir();
        let locked = dir.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

        let reason = blocked_reason(&locked.join("hooks.json")).unwrap();

        assert!(reason.contains("read-only"), "{reason}");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writing_through_a_symlink_keeps_the_link() {
        let dir = scratch_dir();
        let real = dir.join("real.json");
        let link = dir.join("settings.json");
        fs::write(&real, "{}").unwrap();
        symlink(&real, &link).unwrap();

        write_atomic(&link, "{\"a\":1}").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "{\"a\":1}");
        assert_eq!(resolved(&link), fs::canonicalize(&real).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_backup_sits_beside_the_original_and_keeps_its_contents() {
        let dir = scratch_dir();
        let file = dir.join("settings.json");
        fs::write(&file, "original").unwrap();

        let copy = backup(&file).unwrap().unwrap();

        assert_eq!(fs::read_to_string(&copy).unwrap(), "original");
        assert!(copy.to_string_lossy().contains(".mitos-backup-"));
        assert_eq!(backup(&dir.join("absent.json")).unwrap(), None);
        fs::remove_dir_all(dir).unwrap();
    }
}
