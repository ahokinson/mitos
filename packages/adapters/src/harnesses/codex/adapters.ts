import { runTurn } from "@harnesses/codex/appServers.ts";
import { collectCodexHandoff } from "@harnesses/codex/handoffs.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { emitEvent } from "@protocol/channels.ts";
import { AdapterEventKind } from "@protocol/events.ts";
import { Harness } from "@protocol/harnesses.ts";
import type { AdapterDefinition } from "@runtime/adapters.ts";

const completeTurn = async () => {
  emitEvent({ event: AdapterEventKind.TurnComplete });
};

export const codexAdapter: AdapterDefinition = {
  harness: Harness.Codex,
  program: "codex",
  capabilities: {
    headless: true,
    streaming: true,
    detach: true,
    modes: [ThreadMode.Plan, ThreadMode.Build],
    ask_back: true,
  },
  launchArgs: (context, session) =>
    session ? ["resume", session, context] : [context],
  collectHandoff: collectCodexHandoff,
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
