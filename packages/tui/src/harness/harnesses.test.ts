import { expect, test } from "bun:test";

import { detectInstalledHarnesses, KnownHarness } from "@harness/harnesses.ts";

test("detectInstalledHarnesses only returns harnesses Bun.which resolves", () => {
  expect(
    detectInstalledHarnesses((binary: string) =>
      binary === "claude" || binary === "opencode"
        ? `/usr/bin/${binary}`
        : null,
    ),
  ).toEqual([KnownHarness.Claude, KnownHarness.OpenCode]);
});

test("detectInstalledHarnesses returns [] when nothing is on PATH", () => {
  expect(detectInstalledHarnesses(() => null)).toEqual([]);
});

test("detectInstalledHarnesses returns every harness when all are on PATH", () => {
  expect(
    detectInstalledHarnesses((binary: string) => `/usr/bin/${binary}`),
  ).toEqual([
    KnownHarness.Claude,
    KnownHarness.Codex,
    KnownHarness.OpenCode,
    KnownHarness.Hermes,
  ]);
});
