#!/usr/bin/env bun
import { openCodeAdapter } from "@harnesses/opencode/adapters.ts";
import { runAdapter } from "@runtime/adapters.ts";

await runAdapter(openCodeAdapter);
