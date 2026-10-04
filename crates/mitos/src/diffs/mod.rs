mod hunks;
mod lines;
mod payloads;

use serde::Serialize;

pub use payloads::payload_diffs;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub diff: String,
    pub added: usize,
    pub removed: usize,
    pub truncated: usize,
}
