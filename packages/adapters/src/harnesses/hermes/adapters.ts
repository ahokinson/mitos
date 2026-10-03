import { runTurn } from "@harnesses/hermes/acps.ts";
import { collectHermesHandoff } from "@harnesses/hermes/handoffs.ts";
import { emitEvent } from "@protocol/channels.ts";
import { AdapterEventKind } from "@protocol/events.ts";
import { Harness } from "@protocol/harnesses.ts";
import type { AdapterDefinition } from "@runtime/adapters.ts";

const completeTurn = async () => {
  emitEvent({ event: AdapterEventKind.TurnComplete });
};

// Interactive hermes cannot take an initial prompt and keep its REPL, so the
// handoff is printed by Mitos and a linked session resumes in the Mitos
// workspace. Headless turns run over `hermes acp`; no plan mode is offered.
export const hermesAdapter: AdapterDefinition = {
  harness: Harness.Hermes,
  program: "hermes",
  capabilities: {
    headless: true,
    streaming: true,
    detach: true,
    ask_back: true,
  },
  launchArgs: (_context, session) =>
    session ? ["--resume", session, "--no-restore-cwd"] : [],
  collectHandoff: collectHermesHandoff,
  turns: {
    start: (request) =>
      runTurn({
        workdir: request.workdir,
        text: request.initial_context,
        session: null,
      }),
    send: (request, session) =>
      runTurn({
        workdir: request.workdir,
        text: request.text,
        session,
        turnId: request.turn_id,
      }),
    attach: completeTurn,
    detach: completeTurn,
  },
};
