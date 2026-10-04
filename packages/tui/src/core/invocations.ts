/** Path/name of the Mitos core binary the TUI drives. Set by bare `mitos`
 * (see `crates/mitos/src/cli/launchers.rs::launch_tui`) to the exact executable
 * that launched this process; falls back to resolving "mitos" on PATH for
 * standalone/dev invocations. */
function coreProgram(): string {
  return process.env.MITOS_CORE ?? "mitos";
}

export async function run(
  args: string[],
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
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
  try {
    const result = Bun.spawnSync([coreProgram(), ...args], {
      stdout: "pipe",
      stderr: "ignore",
    });
    if (result.exitCode !== 0) return fallback;
    return JSON.parse(result.stdout.toString()) as T;
  } catch {
    return fallback;
  }
}
