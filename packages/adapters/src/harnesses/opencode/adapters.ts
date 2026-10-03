import { collectOpenCodeHandoff } from "@harnesses/opencode/handoffs.ts";
import { runTurn } from "@harnesses/opencode/servers.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { emitEvent } from "@protocol/channels.ts";
import { AdapterEventKind } from "@protocol/events.ts";
import { Harness } from "@protocol/harnesses.ts";
import type { AdapterDefinition } from "@runtime/adapters.ts";

const completeTurn = async () => {
  emitEvent({ event: AdapterEventKind.TurnComplete });
};

export const openCodeAdapter: AdapterDefinition = {
  harness: Harness.OpenCode,
  program: "opencode",
  capabilities: {
    headless: true,
    streaming: true,
    detach: true,
    modes: [ThreadMode.Plan, ThreadMode.Build],
    ask_back: true,
  },
  launchArgs: (context, session) =>
    session
      ? ["--session", session, "--prompt", context]
      : ["--prompt", context],
  collectHandoff: collectOpenCodeHandoff,
  turns: {
    start: (request) =>
      runTurn({
        workdir: request.workdir,
        text: request.initial_context,
        session: null,
        mode: request.mode,
      }),
    send: (request, session) =>
      runTurn({
        workdir: request.workdir,
        text: request.text,
        session,
        mode: request.mode,
        turnId: request.turn_id,
      }),
    attach: completeTurn,
    detach: completeTurn,
  },
};
