#!/usr/bin/env bun
import { hermesAdapter } from "@harnesses/hermes/adapters.ts";
import { runAdapter } from "@runtime/adapters.ts";

await runAdapter(hermesAdapter);
