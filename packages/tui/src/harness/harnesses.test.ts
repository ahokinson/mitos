import { afterEach, expect, spyOn, test } from "bun:test";

import { detectInstalledHarnesses, KnownHarness } from "@harness/harnesses.ts";

afterEach(() => {
  // @ts-expect-error -- spyOn's mock is reassigned onto Bun.which per test
  if (Bun.which.mockRestore) Bun.which.mockRestore();
});

test("detectInstalledHarnesses only returns harnesses Bun.which resolves", () => {
  spyOn(Bun, "which").mockImplementation((binary: string) =>
    binary === "claude" || binary === "opencode" ? `/usr/bin/${binary}` : null,
  );
  expect(detectInstalledHarnesses()).toEqual([
    KnownHarness.Claude,
    KnownHarness.OpenCode,
  ]);
});

test("detectInstalledHarnesses returns [] when nothing is on PATH", () => {
  spyOn(Bun, "which").mockImplementation(() => null);
  expect(detectInstalledHarnesses()).toEqual([]);
});

test("detectInstalledHarnesses returns every harness when all are on PATH", () => {
  spyOn(Bun, "which").mockImplementation(
    (binary: string) => `/usr/bin/${binary}`,
  );
  expect(detectInstalledHarnesses()).toEqual([
    KnownHarness.Claude,
    KnownHarness.Codex,
    KnownHarness.OpenCode,
    KnownHarness.Hermes,
  ]);
});
