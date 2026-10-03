import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadConfig } from "@config/loaders.ts";
import { KnownHarness } from "@harness/harnesses.ts";
import { DEFAULT_LAYOUT_CONFIG } from "@layout/trees.ts";
import { DEFAULT_THEME_NAME } from "@theme/palettes.ts";

let dir: string;
let warnSpy: ReturnType<typeof spyOn>;

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-config-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  warnSpy = spyOn(console, "error").mockImplementation(() => {});
});

afterEach(async () => {
  warnSpy.mockRestore();
  await rm(dir, { recursive: true, force: true });
});

test("missing configuration scaffolds the named default theme and default layout", async () => {
  const handle = await loadConfig(dir);
  expect(handle.config().layout).toEqual(DEFAULT_LAYOUT_CONFIG);
  expect(handle.config().theme?.name).toBe(DEFAULT_THEME_NAME);
  expect(await readFile(join(dir, "mitos.toml"), "utf8")).toContain(
    `name = "${DEFAULT_THEME_NAME}"`,
  );
});

test("loads named widgets, a split layout, and managed handoff settings", async () => {
  await writeFile(join(dir, "stats.hbs"), "{{harness}}");
  await writeFile(
    join(dir, "mitos.toml"),
    `[session]\ndefault_harness = "codex"\n\n[handoff]\nmax_inline_bytes = 65536\n\n[widgets.conversation]\nkind = "conversation"\nfocus_key = "ctrl+1"\n\n[widgets.stats]\ntemplate = "stats.hbs"\n\n[layout]\ndirection = "row"\nchildren = [{ widget = "conversation" }, { widget = "stats", size = 30 }]\n`,
  );
  const handle = await loadConfig(dir);
  expect(handle.config().session?.defaultHarness).toBe(KnownHarness.Codex);
  expect(handle.config().handoff?.maxInlineBytes).toBe(65_536);
  expect(handle.config().layout?.widgets.stats).toEqual({
    templatePath: "stats.hbs",
    template: "{{harness}}",
  });
});

test("an unreadable template view falls back to the default layout", async () => {
  await writeFile(
    join(dir, "mitos.toml"),
    '[widgets.stats]\ntemplate = "missing.hbs"\n\n[layout]\nwidget = "stats"\n',
  );
  const handle = await loadConfig(dir);
  expect(handle.config().layout).toEqual(DEFAULT_LAYOUT_CONFIG);
  expect(warnSpy).toHaveBeenCalled();
});

test("invalid named layout falls back without discarding unrelated session settings", async () => {
  await writeFile(
    join(dir, "mitos.toml"),
    '[session]\ndefault_harness = "claude"\n\n[widgets.bad]\nkind = "missing"\n\n[layout]\nwidget = "bad"\n',
  );
  const handle = await loadConfig(dir);
  expect(handle.config().layout).toEqual(DEFAULT_LAYOUT_CONFIG);
  expect(handle.config().session?.defaultHarness).toBe(KnownHarness.Claude);
  expect(warnSpy).toHaveBeenCalled();
});
