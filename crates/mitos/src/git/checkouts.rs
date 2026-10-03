use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug)]
pub struct Checkout {
    root: PathBuf,
    git: Option<GitRepository>,
}

#[derive(Clone, Debug)]
struct GitRepository {
    root: PathBuf,
    git_dir: PathBuf,
}

impl Checkout {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let requested_root = std::fs::canonicalize(path.as_ref())
            .with_context(|| format!("cannot access {}", path.as_ref().display()))?;
        let git = GitRepository::discover(&requested_root)?;
        let root = git
            .as_ref()
            .map(|repository| repository.root.clone())
            .unwrap_or(requested_root);
        Ok(Self { root, git })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn key(&self) -> String {
        fnv1a(self.root.to_string_lossy().as_bytes())
    }

    pub fn git_dir(&self) -> Option<&Path> {
        self.git
            .as_ref()
            .map(|repository| repository.git_dir.as_path())
    }
}

impl GitRepository {
    fn discover(path: &Path) -> Result<Option<Self>> {
        let output = match Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(path)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("could not inspect Git availability"),
        };
        if !output.status.success() {
            return Ok(None);
        }
        let root = PathBuf::from(String::from_utf8(output.stdout)?.trim());
        let git_dir = run_git(
            &root,
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        Ok(Some(Self {
            root,
            git_dir: PathBuf::from(git_dir),
        }))
    }
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("could not execute git")?;
    if !output.status.success() {
        bail!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn fnv1a(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
