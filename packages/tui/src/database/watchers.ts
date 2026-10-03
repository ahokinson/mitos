import { watch } from "node:fs";
import { basename, dirname } from "node:path";

const DB_CHANGE_DEBOUNCE_MS = 50;

/** Calls `onChange` (debounced) when the SQLite database changes; returns a
 * cleanup. A commit touches `mitos.db`, `-wal`, or `-shm`, so the whole
 * directory is watched and filtered by name prefix. */
export function watchDatabase(
  dbPath: string,
  onChange: () => void,
): () => void {
  const dbName = basename(dbPath);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      onChange();
    }, DB_CHANGE_DEBOUNCE_MS);
  };
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
    if (timer) clearTimeout(timer);
  };
}
