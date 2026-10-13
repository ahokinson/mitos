import { watch } from "node:fs";
import { basename, dirname } from "node:path";

const DB_CHANGE_DEBOUNCE_MS = 50;

/** Calls `onChange` once writes stop for the debounce window; returns a
 * cancel that also clears a pending notification. */
function debounced(onChange: () => void): {
  schedule: () => void;
  cancel: () => void;
} {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    schedule: () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        onChange();
      }, DB_CHANGE_DEBOUNCE_MS);
    },
    cancel: () => {
      if (timer) clearTimeout(timer);
    },
  };
}

/** Calls `onChange` (debounced) when the SQLite database changes; returns a
 * cleanup. A commit touches `mitos.db`, `-wal`, or `-shm`, so the whole
 * directory is watched and filtered by name prefix. */
export function watchDatabase(
  dbPath: string,
  onChange: () => void,
): () => void {
  const dbName = basename(dbPath);
  const { schedule, cancel } = debounced(onChange);
  let watcher: ReturnType<typeof watch> | undefined;
  try {
    watcher = watch(
      dirname(dbPath),
      { persistent: false },
      (_event, fileName) => {
        if (fileName && String(fileName).startsWith(dbName)) schedule();
      },
    );
  } catch {
    // Ctrl+L remains an explicit recovery path when the host cannot watch files.
  }
  return () => {
    watcher?.close();
    cancel();
  };
}

/** Calls `onChange` (debounced) when anything under a harness's stored
 * history changes; returns a cleanup. Session files can land anywhere in
 * the tree, so the watch is recursive and unfiltered. A host that cannot
 * watch recursively is skipped, the same recovery path as the database
 * watch. */
export function watchHistory(root: string, onChange: () => void): () => void {
  const { schedule, cancel } = debounced(onChange);
  let watcher: ReturnType<typeof watch> | undefined;
  try {
    watcher = watch(root, { persistent: false, recursive: true }, () =>
      schedule(),
    );
  } catch {
    // Ctrl+L remains an explicit recovery path when the host cannot watch files.
  }
  return () => {
    watcher?.close();
    cancel();
  };
}
