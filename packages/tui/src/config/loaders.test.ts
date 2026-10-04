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

test("a config directory that cannot be created runs on defaults", async () => {
  await writeFile(join(dir, "blocker"), "");
  const handle = await loadConfig(join(dir, "blocker", "nested"));
  expect(handle.config().layout).toEqual(DEFAULT_LAYOUT_CONFIG);
  expect(warnSpy).toHaveBeenCalled();
});

test("an unparseable config file runs on defaults", async () => {
  await writeFile(join(dir, "mitos.toml"), "this is = = not toml");
  const handle = await loadConfig(dir);
  expect(handle.config().layout).toEqual(DEFAULT_LAYOUT_CONFIG);
  expect(warnSpy).toHaveBeenCalled();
});

test("a theme name that is neither built in nor declared is dropped", async () => {
  await writeFile(join(dir, "mitos.toml"), '[theme]\nname = "nonexistent"\n');
  const handle = await loadConfig(dir);
  expect(handle.config().theme?.name).toBeUndefined();
  expect(warnSpy).toHaveBeenCalled();
});

test("a declared custom theme can be selected by name", async () => {
  await writeFile(
    join(dir, "night.toml"),
    'chrome = "none"\nbackgroundChrome = "#000000"\nbackgroundSelection = "#111111"\ntext = "#ffffff"\ntextMuted = "#eeeeee"\ntextDim = "#dddddd"\naccent = "#0000ff"\nsuccess = "#00ff00"\nwarning = "#ffff00"\nerr = "#ff0000"\n',
  );
  await writeFile(
    join(dir, "mitos.toml"),
    '[themes]\nnight = "night.toml"\n\n[theme]\nname = "night"\n',
  );
  const handle = await loadConfig(dir);
  expect(handle.config().theme?.name).toBe("night");
  expect(handle.customThemes().night?.accent).toBe("#0000ff");
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
