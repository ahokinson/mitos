import { collectClaudeHandoff } from "@harnesses/claude/handoffs.ts";
import { replayTranscript, runTurn } from "@harnesses/claude/turns.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { Harness } from "@protocol/harnesses.ts";
import type { AdapterDefinition } from "@runtime/adapters.ts";

export const claudeAdapter: AdapterDefinition = {
  harness: Harness.Claude,
  program: "claude",
  capabilities: {
    headless: true,
    streaming: true,
    detach: true,
    modes: [ThreadMode.Plan, ThreadMode.Build],
    ask_back: false,
  },
  launchArgs: (context, session) =>
    session ? ["--resume", session, context] : [context],
  collectHandoff: collectClaudeHandoff,
  turns: {
    start: (request) =>
      runTurn({
        workdir: request.workdir,
        text: request.initial_context,
        mode: request.mode,
      }),
    send: (request, session) =>
      runTurn({
        workdir: request.workdir,
        text: request.text,
        resume: session,
        turnId: request.turn_id,
        mode: request.mode,
      }),
    attach: replayTranscript,
    detach: async () => {},
  },
};
