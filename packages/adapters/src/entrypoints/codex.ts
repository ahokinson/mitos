#!/usr/bin/env bun
import { codexAdapter } from "@harnesses/codex/adapters.ts";
import { runAdapter } from "@runtime/adapters.ts";

await runAdapter(codexAdapter);
