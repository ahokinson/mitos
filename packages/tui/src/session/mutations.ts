import { run, runJson } from "@core/invocations.ts";
import type { HookInitOutcome, HookStatus } from "@session/hooks.ts";
import { Decision, type Reply } from "@session/requests.ts";
import type { CompactMode, Thread, ThreadMode } from "@session/threads.ts";

/** Changes to Mitos state. Reads go through `@database/views.ts`. */

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

export async function compactThread(
  threadId: string,
  mode: CompactMode,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "compact",
    threadId,
    "--mode",
    mode,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not compact thread",
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

/** Ask OpenCode to stop the thread's running turn. */
export async function abortThread(threadId: string): Promise<void> {
  const { exitCode, stdout, stderr } = await run(["thread", "abort", threadId]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not abort turn");
}

/** Forks the thread's OpenCode session into a new thread. */
export function forkThread(threadId: string): Promise<Thread> {
  return runJson<Thread>(["thread", "fork", threadId]);
}

/** Publishes the thread's OpenCode session; returns the share URL. */
export async function shareThread(threadId: string): Promise<string> {
  const { exitCode, stdout, stderr } = await run(["thread", "share", threadId]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not share thread");
  return stdout.trim();
}

/** The files the thread's OpenCode session changed, as summary lines. */
export async function diffThread(threadId: string): Promise<string[]> {
  const { exitCode, stdout, stderr } = await run(["thread", "diff", threadId]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not read diff");
  return stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

/** Sets the thread's agent; `null` returns it to the mode's default. */
export async function setAgent(
  threadId: string,
  agent: string | null,
): Promise<void> {
  const args = ["thread", "agent", threadId];
  if (agent) args.push(agent);
  const { exitCode, stdout, stderr } = await run(args);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not set agent");
}

/** Sets the thread's model reference; `null` returns it to the default. */
export async function setModel(
  threadId: string,
  model: string | null,
): Promise<void> {
  const args = ["thread", "model", threadId];
  if (model) args.push(model);
  const { exitCode, stdout, stderr } = await run(args);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not set model");
}

/** The agents the thread's OpenCode server offers, as `name: description` lines. */
export async function listAgents(threadId: string): Promise<string[]> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "agents",
    threadId,
  ]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not list agents");
  return stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

/** The default model per provider, as `provider/model` lines. */
export async function listModels(threadId: string): Promise<string[]> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "models",
    threadId,
  ]);
  if (exitCode !== 0)
    throw new Error(stderr.trim() || stdout.trim() || "Could not list models");
  return stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

/** Runs an OpenCode slash command in the thread's session. */
export async function runCommand(
  threadId: string,
  command: string,
  args: readonly string[],
): Promise<string> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "command",
    threadId,
    command,
    ...args,
  ]);
  if (exitCode !== 0)
    throw new Error(
      stderr.trim() || stdout.trim() || "Could not run the command",
    );
  return stdout.trim();
}

export async function answerRequest(
  threadId: string,
  requestId: string,
  reply: Reply,
): Promise<void> {
  const { exitCode, stdout, stderr } = await run([
    "thread",
    "answer",
    threadId,
    "--request",
    requestId,
    ...(reply.decision === Decision.Approve ? ["--approve"] : []),
    ...(reply.decision === Decision.Deny ? ["--deny"] : []),
    ...(reply.text?.trim() ? ["--text", reply.text] : []),
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
