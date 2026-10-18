import { expect, test } from "bun:test";

import { detectInstalledHarnesses, KnownHarness } from "@harness/harnesses.ts";

test("detectInstalledHarnesses only returns harnesses Bun.which resolves", () => {
  expect(
    detectInstalledHarnesses((binary: string) =>
      binary === "claude" || binary === "opencode"
        ? `/usr/bin/${binary}`
        : null,
    ),
  ).toEqual([KnownHarness.OpenCode, KnownHarness.Claude]);
});

test("detectInstalledHarnesses returns [] when nothing is on PATH", () => {
  expect(detectInstalledHarnesses(() => null)).toEqual([]);
});

test("detectInstalledHarnesses returns every harness when all are on PATH", () => {
  expect(
    detectInstalledHarnesses((binary: string) => `/usr/bin/${binary}`),
  ).toEqual([
    KnownHarness.OpenCode,
    KnownHarness.Claude,
    KnownHarness.Codex,
    KnownHarness.Hermes,
  ]);
});

test("OpenCode is preferred when several harnesses are installed", () => {
  const installed = detectInstalledHarnesses(
    (binary: string) => `/usr/bin/${binary}`,
  );
  expect(installed[0]).toBe(KnownHarness.OpenCode);
});
