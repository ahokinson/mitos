/** Path/name of the Mitos core binary the TUI drives. Set by bare `mitos`
 * (see `crates/mitos/src/cli/launchers.rs::launch_tui`) to the exact executable
 * that launched this process; falls back to resolving "mitos" on PATH for
 * standalone/dev invocations. */
function coreProgram(): string {
  return process.env.MITOS_CORE ?? "mitos";
}

/** Every core invocation opens the store, and a SQLite open-close in WAL
 * mode churns `mitos.db`, `-wal` and `-shm` — the very files the UI watches
 * for external writes. Without a shadow, each read looks like a write to the
 * UI's own watcher and re-triggers the refresh that caused it: a feedback
 * loop that keeps the UI thread busy with synchronous spawns. */
const CORE_READ_SHADOW_MS = 100;

let readsInFlight = 0;
let lastReadFinishedAt = 0;

function readStarted(): void {
  readsInFlight += 1;
}

function readFinished(): void {
  readsInFlight -= 1;
  lastReadFinishedAt = performance.now();
}

/** True while the TUI's own core reads may still be churning the watched
 * database files; a watcher must not count these as external writes. */
export function coreReadsRecent(): boolean {
  return (
    readsInFlight > 0 ||
    performance.now() - lastReadFinishedAt < CORE_READ_SHADOW_MS
  );
}

export async function run(
  args: string[],
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  readStarted();
  try {
    const process = Bun.spawn([coreProgram(), ...args], {
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exitCode, stdout, stderr] = await Promise.all([
      process.exited,
      new Response(process.stdout).text(),
      new Response(process.stderr).text(),
    ]);
    return { exitCode, stdout, stderr };
  } finally {
    readFinished();
  }
}

export async function runJson<T>(args: string[]): Promise<T> {
  const { exitCode, stdout, stderr } = await run([...args, "--json"]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Mitos command failed");
  return JSON.parse(stdout) as T;
}

/** A read the UI needs answered before it draws. Blocks for the few
 * milliseconds a `mitos` invocation takes; any failure yields `fallback`, so
 * a read that goes wrong shows nothing instead of crashing the screen. */
export function readJson<T>(args: string[], fallback: T): T {
  readStarted();
  try {
    const result = Bun.spawnSync([coreProgram(), ...args], {
      stdout: "pipe",
      stderr: "ignore",
    });
    if (result.exitCode !== 0) return fallback;
    return JSON.parse(result.stdout.toString()) as T;
  } catch {
    return fallback;
  } finally {
    readFinished();
  }
}
