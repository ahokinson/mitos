import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { pruneEmptyThreads } from "@session/exits.ts";

let dir: string;
let savedCore: string | undefined;

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-exits-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  savedCore = process.env.MITOS_CORE;
  const script = join(dir, "core.sh");
  await writeFile(
    script,
    `#!/bin/sh
if [ "$1" = view ]; then
  case "$3" in
    empty*) echo true ;;
    *) echo false ;;
  esac
  exit 0
fi
here=$(dirname "$0")
line=$(printf '%s|' "$@")
echo "$line" >> "$here/calls.log"
case "$3" in
  *broken) exit 1 ;;
esac
`,
  );
  await chmod(script, 0o755);
  process.env.MITOS_CORE = script;
});

afterEach(async () => {
  if (savedCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = savedCore;
  await rm(dir, { recursive: true, force: true });
});

const log = () => join(dir, "calls.log");

test("only threads that are still empty are deleted", async () => {
  await pruneEmptyThreads(["empty-1", "used-1", "empty-2"]);
  const calls = (await readFile(log(), "utf8")).trim().split("\n").sort();
  expect(calls).toEqual(["thread|delete|empty-1|", "thread|delete|empty-2|"]);
});

test("a failed delete does not stop the others", async () => {
  await pruneEmptyThreads(["empty-broken", "empty-ok"]);
  const calls = (await readFile(log(), "utf8")).trim().split("\n");
  expect(calls).toHaveLength(2);
});
