import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { readJson, run, runJson } from "@core/invocations.ts";

let dir: string;
let savedCore: string | undefined;

async function fakeCore(script: string): Promise<void> {
  const path = join(dir, "core.sh");
  await writeFile(path, `#!/bin/sh\n${script}\n`);
  await chmod(path, 0o755);
  process.env.MITOS_CORE = path;
}

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-core-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  savedCore = process.env.MITOS_CORE;
});

afterEach(async () => {
  if (savedCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = savedCore;
  await rm(dir, { recursive: true, force: true });
});

test("run captures exit code, stdout and stderr", async () => {
  await fakeCore('echo "out $*"; echo err >&2; exit 3');
  expect(await run(["a", "b"])).toEqual({
    exitCode: 3,
    stdout: "out a b\n",
    stderr: "err\n",
  });
});

test("runJson appends --json and parses stdout", async () => {
  await fakeCore('echo "{\\"args\\":\\"$*\\"}"');
  expect(await runJson<{ args: string }>(["view"])).toEqual({
    args: "view --json",
  });
});

test("runJson throws stderr, then stdout, then a default message", async () => {
  await fakeCore("echo boom >&2; exit 1");
  await expect(runJson([])).rejects.toThrow("boom");
  await fakeCore("echo only-out; exit 1");
  await expect(runJson([])).rejects.toThrow("only-out");
  await fakeCore("exit 1");
  await expect(runJson([])).rejects.toThrow("Mitos command failed");
});

test("readJson parses output and falls back on failure", async () => {
  await fakeCore("echo '[1,2]'");
  expect(readJson<number[]>([], [])).toEqual([1, 2]);
  await fakeCore("exit 1");
  expect(readJson<number[]>([], [9])).toEqual([9]);
  await fakeCore("echo not-json");
  expect(readJson<number[]>([], [9])).toEqual([9]);
});

test("readJson falls back when the program cannot be spawned", () => {
  process.env.MITOS_CORE = join(dir, "missing");
  expect(readJson<number[]>([], [7])).toEqual([7]);
});

test("the core program defaults to mitos on PATH", async () => {
  delete process.env.MITOS_CORE;
  const original = process.env.PATH;
  process.env.PATH = dir;
  try {
    expect(readJson<number[]>([], [5])).toEqual([5]);
  } finally {
    process.env.PATH = original;
  }
});
