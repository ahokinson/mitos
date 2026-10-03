import type { HookInitOutcome, HookStatus } from "@session/hooks.ts";
import type { Thread, ThreadMode } from "@session/threads.ts";

/** Path/name of the Mitos core binary the TUI drives. Set by bare `mitos`
 * (see `crates/mitos/src/cli/launchers.rs::launch_tui`) to the exact executable that
 * launched this process; falls back to resolving "mitos" on PATH for
 * standalone/dev invocations. Mutations only — reads go through
 * `@database/databases.ts`'s direct `bun:sqlite` access instead. */
const core = process.env.MITOS_CORE ?? "mitos";

async function run(
  args: string[],
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const process = Bun.spawn([core, ...args], {
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

async function runJson<T>(args: string[]): Promise<T> {
  const { exitCode, stdout, stderr } = await run([...args, "--json"]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Mitos command failed");
  return JSON.parse(stdout) as T;
}

export function createThread(harness: string | null): Promise<Thread> {
  const args = ["thread", "new"];
  if (harness) args.push("--harness", harness);
  const workspace = process.env.MITOS_WORKSPACE_ROOT;
  if (workspace) args.push("--workspace", workspace);
  return runJson<Thread>(args);
}

export async function sendMessage(
  threadId: string,
  message: string,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "send",
    threadId,
    "--message",
    message,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not queue message",
    );
}

export async function reassignHarness(
  threadId: string,
  to: string,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "reassign",
    threadId,
    "--to",
    to,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not reassign harness",
    );
}

export async function archiveThread(threadId: string): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "archive",
    threadId,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not archive thread",
    );
}

/** Permanently removes the thread and all its rows — unlike
 * `archiveThread`, not recoverable. Called from the `/delete` command's own
 * two-step typed confirmation (`commands/handlers/threads.ts`). */
export async function deleteThread(threadId: string): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "delete",
    threadId,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not delete thread",
    );
}

export async function setMode(
  threadId: string,
  mode: ThreadMode,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "mode",
    threadId,
    mode,
  ]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not set mode");
}

export async function answerRequest(
  threadId: string,
  requestId: string,
  response: unknown,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "answer",
    threadId,
    "--request",
    requestId,
    "--response",
    JSON.stringify(response),
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not answer request",
    );
}

export function hooksStatus(): Promise<HookStatus[]> {
  return runJson<HookStatus[]>(["hooks", "status"]);
}

/** An empty list installs every harness's hook. */
export function hooksInit(harnesses: string[]): Promise<HookInitOutcome[]> {
  return runJson<HookInitOutcome[]>([
    "hooks",
    "init",
    ...harnesses.flatMap((harness) => ["--harness", harness]),
  ]);
}

export async function noteThread(
  threadId: string,
  fields: { note?: string; decisions?: string[]; questions?: string[] },
): Promise<void> {
  const args = ["thread", "note", threadId];
  if (fields.note) args.push("--note", fields.note);
  for (const decision of fields.decisions ?? [])
    args.push("--decision", decision);
  for (const question of fields.questions ?? [])
    args.push("--question", question);
  const { exitCode, stdout, stderr } = await run(args);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not note thread");
}
