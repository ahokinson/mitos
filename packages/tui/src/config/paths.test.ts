import { afterEach, beforeEach, expect, test } from "bun:test";
import { homedir } from "node:os";
import { join } from "node:path";

import {
  expandHome,
  resolveConfigDir,
  resolveConfigPath,
  resolveDbPath,
  resolveTemplatePath,
  resolveThemePath,
} from "@config/paths.ts";

const configPath = "/tmp/mitos-config/mitos.toml";

let savedXdg: string | undefined;

beforeEach(() => {
  savedXdg = process.env.XDG_CONFIG_HOME;
});

afterEach(() => {
  if (savedXdg === undefined) delete process.env.XDG_CONFIG_HOME;
  else process.env.XDG_CONFIG_HOME = savedXdg;
});

test("an explicit override directory is used verbatim", () => {
  expect(resolveConfigDir("/srv/mitos")).toBe("/srv/mitos");
  expect(resolveConfigPath("/srv/mitos")).toBe("/srv/mitos/mitos.toml");
  expect(resolveDbPath("/srv/mitos")).toBe("/srv/mitos/mitos.db");
});

test("the config directory follows XDG_CONFIG_HOME, then ~/.config", () => {
  process.env.XDG_CONFIG_HOME = "/xdg";
  expect(resolveConfigDir()).toBe("/xdg/mitos");
  process.env.XDG_CONFIG_HOME = "";
  expect(resolveConfigDir()).toBe(join(homedir(), ".config", "mitos"));
  delete process.env.XDG_CONFIG_HOME;
  expect(resolveConfigDir()).toBe(join(homedir(), ".config", "mitos"));
});

test("expandHome handles bare tilde, tilde paths, and other paths", () => {
  expect(expandHome("~")).toBe(homedir());
  expect(expandHome("~/a/b")).toBe(join(homedir(), "a/b"));
  expect(expandHome("~other/a")).toBe("~other/a");
  expect(expandHome("/abs")).toBe("/abs");
});

test("theme paths resolve like template paths", () => {
  expect(resolveThemePath("night.toml", configPath)).toBe(
    "/tmp/mitos-config/night.toml",
  );
});

test("template paths resolve relative to mitos.toml", () => {
  expect(resolveTemplatePath("widgets/usage.hbs", configPath)).toBe(
    "/tmp/mitos-config/widgets/usage.hbs",
  );
});

test("template paths preserve absolute paths and expand home", () => {
  expect(resolveTemplatePath("/var/tmp/usage.hbs", configPath)).toBe(
    "/var/tmp/usage.hbs",
  );
  expect(resolveTemplatePath("~/widgets/usage.hbs", configPath)).toBe(
    join(homedir(), "widgets/usage.hbs"),
  );
});
