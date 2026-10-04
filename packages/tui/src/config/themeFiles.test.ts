import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadCustomThemes } from "@config/themeFiles.ts";

let dir: string;
let warnSpy: ReturnType<typeof spyOn>;

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-theme-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  warnSpy = spyOn(console, "error").mockImplementation(() => {});
});

afterEach(async () => {
  warnSpy.mockRestore();
  await rm(dir, { recursive: true, force: true });
});

const validTheme = `chrome = "framed"
backgroundChrome = "#000000"
backgroundSelection = "#111111"
text = "#ffffff"
textMuted = "#eeeeee"
textDim = "#dddddd"
accent = "#0000ff"
success = "#00ff00"
warning = "#ffff00"
err = "#ff0000"
`;

test("loads a valid theme relative to the config file", async () => {
  await writeFile(join(dir, "night.toml"), validTheme);
  const themes = await loadCustomThemes(
    { night: "night.toml" },
    join(dir, "mitos.toml"),
  );
  expect(themes.night?.accent).toBe("#0000ff");
  expect(warnSpy).not.toHaveBeenCalled();
});

test("skips a theme missing required tokens", async () => {
  await writeFile(join(dir, "bad.toml"), 'chrome = "framed"\n');
  const themes = await loadCustomThemes(
    { bad: "bad.toml" },
    join(dir, "mitos.toml"),
  );
  expect(themes).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(1);
});

test("skips unreadable and non-toml paths", async () => {
  const themes = await loadCustomThemes(
    { gone: "gone.toml", wrong: "wrong.json" },
    join(dir, "mitos.toml"),
  );
  expect(themes).toEqual({});
  expect(warnSpy).toHaveBeenCalledTimes(2);
});
