import { expect, test } from "bun:test";

import { hermesAdapter } from "@harnesses/hermes/adapters.ts";
import { openCodeAdapter } from "@harnesses/opencode/adapters.ts";
import { ThreadMode } from "@protocol/actions.ts";
import { Harness } from "@protocol/harnesses.ts";

test("opencode launches interactively with the context as its prompt", () => {
  expect(openCodeAdapter.harness).toBe(Harness.OpenCode);
  expect(openCodeAdapter.program).toBe("opencode");
  expect(openCodeAdapter.launchArgs("ctx", null)).toEqual(["--prompt", "ctx"]);
  expect(openCodeAdapter.launchArgs("ctx", "ses_1")).toEqual([
    "--session",
    "ses_1",
    "--prompt",
    "ctx",
  ]);
});

test("hermes resumes a linked session and never takes the context as an arg", () => {
  expect(hermesAdapter.harness).toBe(Harness.Hermes);
  expect(hermesAdapter.program).toBe("hermes");
  expect(hermesAdapter.launchArgs("ctx", null)).toEqual([]);
  expect(hermesAdapter.launchArgs("ctx", "sess-1")).toEqual([
    "--resume",
    "sess-1",
    "--no-restore-cwd",
  ]);
});

test("opencode is headless with plan and build modes and ask-back", () => {
  expect(openCodeAdapter.capabilities).toEqual({
    headless: true,
    streaming: true,
    detach: true,
    modes: [ThreadMode.Plan, ThreadMode.Build],
    ask_back: true,
  });
  expect(openCodeAdapter.turns).toBeDefined();
});

test("hermes is headless and build-only, so plan reassignment is refused", () => {
  expect(hermesAdapter.capabilities).toEqual({
    headless: true,
    streaming: true,
    detach: true,
    ask_back: true,
  });
  expect(hermesAdapter.capabilities.modes).toBeUndefined();
  expect(hermesAdapter.turns).toBeDefined();
});
