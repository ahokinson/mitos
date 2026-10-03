import { expect, test } from "bun:test";
import { homedir } from "node:os";
import { join } from "node:path";

import { resolveTemplatePath } from "@config/paths.ts";

const configPath = "/tmp/mitos-config/mitos.toml";

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
