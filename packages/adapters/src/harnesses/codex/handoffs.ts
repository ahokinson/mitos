import { join } from "node:path";

import { collectJsonlHandoff } from "@history/transcripts/collectors.ts";
import type { CollectHandoffRequest } from "@protocol/actions.ts";
import type { HandoffResponse } from "@protocol/responses.ts";

export function collectCodexHandoff(
  request: CollectHandoffRequest,
): Promise<HandoffResponse> {
  const home = process.env.CODEX_HOME ?? join(process.env.HOME ?? "", ".codex");
  return collectJsonlHandoff(
    request,
    "Codex session transcript",
    join(home, "sessions"),
  );
}
