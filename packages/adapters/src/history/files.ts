import type { Dirent } from "node:fs";
import { readdir, stat } from "node:fs/promises";
import { join } from "node:path";

export type Candidate = { path: string; modified: number };

const MAX_DIRECTORIES = 256;

/** Walks `root` for files ending in `extension`, keeping the `keep` most
 * recently modified. The result's order is unspecified. */
export async function discoverFiles(
  root: string,
  extension: string,
  keep: number,
): Promise<Candidate[]> {
  const result: Candidate[] = [];
  const directories = [root];
  let visited = 0;
  while (directories.length > 0 && visited < MAX_DIRECTORIES) {
    const directory = directories.pop();
    if (!directory) continue;
    visited += 1;
    let entries: Dirent[];
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) directories.push(path);
      if (!entry.isFile() || !entry.name.endsWith(extension)) continue;
      try {
        retainNewest(
          result,
          { path, modified: (await stat(path)).mtimeMs },
          keep,
        );
      } catch {
        // Files can vanish mid-walk.
      }
    }
  }
  return result;
}

function retainNewest(
  candidates: Candidate[],
  candidate: Candidate,
  keep: number,
): void {
  candidates.push(candidate);
  if (candidates.length <= keep) return;
  candidates.sort((left, right) => right.modified - left.modified);
  candidates.length = keep;
}
