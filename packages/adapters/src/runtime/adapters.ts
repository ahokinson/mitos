import { stringValue } from "@json/scalars.ts";
import {
  Action,
  type AttachThreadRequest,
  type CollectHandoffRequest,
  type DetachThreadRequest,
  PROTOCOL_VERSION,
  type SendMessageRequest,
  type StartThreadRequest,
} from "@protocol/actions.ts";
import { readRequest, respond } from "@protocol/channels.ts";
import type { Harness } from "@protocol/harnesses.ts";
import {
  type Capabilities,
  type HandoffResponse,
  ResponseKind,
} from "@protocol/responses.ts";

export type TurnHandlers = {
  start(request: StartThreadRequest): Promise<void>;
  send(request: SendMessageRequest, session: string): Promise<void>;
  attach(request: AttachThreadRequest): Promise<void>;
  detach(request: DetachThreadRequest): Promise<void>;
};

export type AdapterDefinition = {
  harness: Harness;
  program: string;
  capabilities: Capabilities;
  launchArgs(context: string, session: string | null): string[];
  collectHandoff(
    request: CollectHandoffRequest,
  ): HandoffResponse | Promise<HandoffResponse>;
  turns?: TurnHandlers;
};

export async function runAdapter(adapter: AdapterDefinition): Promise<void> {
  const request = await readRequest();
  const name = `mitos-${adapter.harness}`;
  if (request.harness !== adapter.harness)
    throw new Error(`${name} received the wrong harness`);

  switch (request.action) {
    case Action.Negotiate:
      respond({
        protocol_version: PROTOCOL_VERSION,
        kind: ResponseKind.Capabilities,
        capabilities: adapter.capabilities,
      });
      return;
    case Action.PrepareLaunch: {
      const session = stringValue(request.native_session);
      respond({
        protocol_version: PROTOCOL_VERSION,
        kind: ResponseKind.Launch,
        program: adapter.program,
        args: adapter.launchArgs(request.context, session),
        native_session: session,
      });
      return;
    }
    case Action.CollectHandoff:
      respond(await adapter.collectHandoff(request));
      return;
  }

  const turns = adapter.turns;
  if (!turns) throw new Error(`${name} does not support ${request.action}`);
  switch (request.action) {
    case Action.StartThread:
      return turns.start(request);
    case Action.SendMessage: {
      const session = stringValue(request.native_session);
      if (!session) throw new Error("send_message requires a native_session");
      return turns.send(request, session);
    }
    case Action.AttachThread:
      return turns.attach(request);
    case Action.DetachThread:
      return turns.detach(request);
  }
}
