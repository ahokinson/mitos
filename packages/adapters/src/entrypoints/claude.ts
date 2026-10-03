#!/usr/bin/env bun
import { claudeAdapter } from "@harnesses/claude/adapters.ts";
import { runAdapter } from "@runtime/adapters.ts";

await runAdapter(claudeAdapter);
