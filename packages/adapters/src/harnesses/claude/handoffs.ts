import { join } from "node:path";
import { collectJsonlHandoff } from "@history/transcripts/collectors.ts";
import type { CollectHandoffRequest } from "@protocol/actions.ts";
import type { HandoffResponse } from "@protocol/responses.ts";

export function collectClaudeHandoff(
  request: CollectHandoffRequest,
): Promise<HandoffResponse> {
  const home =
    process.env.CLAUDE_CONFIG_DIR ?? join(process.env.HOME ?? "", ".claude");
  return collectJsonlHandoff(
    request,
    "Claude session transcript",
    join(home, "projects"),
  );
}
