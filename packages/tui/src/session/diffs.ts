/** Mirrors `crates/mitos/src/diffs/mod.rs::FileDiff`. */
export type FileDiff = {
  path: string;
  diff: string;
  added: number;
  removed: number;
  truncated: number;
};
