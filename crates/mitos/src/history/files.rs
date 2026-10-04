use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_DIRECTORIES: usize = 256;

pub struct Candidate {
    pub path: PathBuf,
    pub modified: SystemTime,
}

/// Walks `root` for files ending in `extension`, keeping the `keep` most
/// recently modified. The result's order is unspecified.
pub fn discover_files(root: &Path, extension: &str, keep: usize) -> Vec<Candidate> {
    let mut result: Vec<Candidate> = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    let mut visited = 0;
    while visited < MAX_DIRECTORIES {
        let Some(directory) = directories.pop() else {
            break;
        };
        visited += 1;
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                directories.push(path);
                continue;
            }
            if !kind.is_file() || !entry.file_name().to_string_lossy().ends_with(extension) {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
                continue;
            };
            retain_newest(&mut result, Candidate { path, modified }, keep);
        }
    }
    result
}

fn retain_newest(candidates: &mut Vec<Candidate>, candidate: Candidate, keep: usize) {
    candidates.push(candidate);
    if candidates.len() <= keep {
        return;
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    candidates.truncate(keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_matching_files_across_nested_directories() {
        let root = std::env::temp_dir().join(format!("mitos-files-{}", crate::domain::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("one.jsonl"), "").unwrap();
        std::fs::write(nested.join("two.jsonl"), "").unwrap();
        std::fs::write(nested.join("skip.txt"), "").unwrap();

        let mut found: Vec<String> = discover_files(&root, ".jsonl", 10)
            .into_iter()
            .map(|candidate| candidate.path.file_name().unwrap().to_string_lossy().into())
            .collect();
        found.sort();
        assert_eq!(found, ["one.jsonl", "two.jsonl"]);
        assert_eq!(discover_files(&root, ".jsonl", 1).len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_root_yields_nothing() {
        assert!(discover_files(Path::new("/nonexistent/mitos"), ".jsonl", 4).is_empty());
    }
}
