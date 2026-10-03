import { collectClaudeHandoff } from "@harnesses/claude/handoffs.ts";
import {
  controlResponseFor,
  type PendingControl,
  processStreamLine,
  requestFromControl,
} from "@harnesses/claude/streams.ts";
import type { AttachThreadRequest } from "@protocol/actions.ts";
import { Action, PROTOCOL_VERSION, ThreadMode } from "@protocol/actions.ts";
import { emitEvent, readAnswers, readLines } from "@protocol/channels.ts";
import { AdapterEventKind } from "@protocol/events.ts";
import { Harness } from "@protocol/harnesses.ts";
import { MessageRole } from "@protocol/roles.ts";

enum ClaudePermissionMode {
  Plan = "plan",
  BypassPermissions = "bypassPermissions",
}

export async function replayTranscript(
  request: AttachThreadRequest,
): Promise<void> {
  const handoff = await collectClaudeHandoff({
    protocol_version: PROTOCOL_VERSION,
    action: Action.CollectHandoff,
    harness: Harness.Claude,
    workdir: request.workdir,
    launched_at: new Date(0).toISOString(),
    native_session: request.native_session,
  });
  for (const message of handoff.transcript?.messages ?? []) {
    emitEvent({
      event:
        message.role === MessageRole.Assistant
          ? AdapterEventKind.AssistantMessage
          : AdapterEventKind.Status,
      role: message.role,
      content: message.text,
    });
  }
  if (handoff.usage)
    emitEvent({ event: AdapterEventKind.Usage, usage: handoff.usage });
  emitEvent({ event: AdapterEventKind.TurnComplete });
}

export async function runTurn(options: {
  workdir: string;
  text: string;
  resume?: string;
  turnId?: string;
  mode: ThreadMode;
}): Promise<void> {
  const args = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--permission-prompts",
    "none",
    "--permission-mode",
    options.mode === ThreadMode.Plan
      ? ClaudePermissionMode.Plan
      : ClaudePermissionMode.BypassPermissions,
  ];
  if (options.resume) args.push("--resume", options.resume);

  const proc = Bun.spawn(["claude", ...args], {
    cwd: options.workdir,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "inherit",
  });
  const send = (message: unknown) => {
    proc.stdin.write(`${JSON.stringify(message)}\n`);
    proc.stdin.flush();
  };
  send({ type: "user", message: { role: "user", content: options.text } });

  const pending = new Map<string, PendingControl>();
  // Ends when core closes our stdin after `turn_complete`.
  void (async () => {
    for await (const answer of readAnswers()) {
      const control = pending.get(answer.request_id);
      if (!control) continue;
      pending.delete(answer.request_id);
      send(controlResponseFor(control, answer.response));
    }
  })();

  for await (const line of readLines(proc.stdout)) {
    if (!line.trim()) continue;
    let record: unknown;
    try {
      record = JSON.parse(line);
    } catch {
      continue;
    }
    const control = requestFromControl(record, options.turnId);
    if (control) {
      pending.set(control.pending.cliRequestId, control.pending);
      emitEvent(control.event);
      continue;
    }
    const events = processStreamLine(record, options.turnId);
    for (const event of events) emitEvent(event);
    // One turn per run: end input once the CLI reports a result.
    if (events.some((event) => event.event === AdapterEventKind.TurnComplete))
      proc.stdin.end();
  }

  const exitCode = await proc.exited;
  if (exitCode !== 0) {
    emitEvent({
      event: AdapterEventKind.Error,
      turn_id: options.turnId,
      content: `claude exited with code ${exitCode}`,
    });
  }
}
