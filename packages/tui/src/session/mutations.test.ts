import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  answerRequest,
  archiveThread,
  compactThread,
  createThread,
  deleteThread,
  hooksInit,
  hooksStatus,
  noteThread,
  reassignHarness,
  sendMessage,
  setMode,
} from "@session/mutations.ts";
import { Decision } from "@session/requests.ts";
import { CompactMode, ThreadMode } from "@session/threads.ts";

let dir: string;
let savedCore: string | undefined;
let savedWorkspace: string | undefined;

/** The fake core logs its argv and replays whatever control files sit beside
 * it: `fail`, `err`, `out`. */
beforeEach(async () => {
  dir = join(tmpdir(), `mitos-mutations-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
  savedCore = process.env.MITOS_CORE;
  savedWorkspace = process.env.MITOS_WORKSPACE_ROOT;
  const script = join(dir, "core.sh");
  await writeFile(
    script,
    `#!/bin/sh
here=$(dirname "$0")
printf '%s|' "$@" >> "$here/calls.log"
echo >> "$here/calls.log"
[ -f "$here/err" ] && cat "$here/err" >&2
[ -f "$here/out" ] && cat "$here/out"
[ -f "$here/fail" ] && exit 1
exit 0
`,
  );
  await chmod(script, 0o755);
  process.env.MITOS_CORE = script;
});

afterEach(async () => {
  if (savedCore === undefined) delete process.env.MITOS_CORE;
  else process.env.MITOS_CORE = savedCore;
  if (savedWorkspace === undefined) delete process.env.MITOS_WORKSPACE_ROOT;
  else process.env.MITOS_WORKSPACE_ROOT = savedWorkspace;
  await rm(dir, { recursive: true, force: true });
});

async function calls(): Promise<string[]> {
  return (await readFile(join(dir, "calls.log"), "utf8")).trim().split("\n");
}

async function control(name: "fail" | "err" | "out", text = ""): Promise<void> {
  await writeFile(join(dir, name), text);
}

const voidMutations: [string, () => Promise<void>, string, string][] = [
  [
    "sendMessage",
    () => sendMessage("t1", "hi there"),
    "thread|send|t1|--message|hi there|",
    "Could not queue message",
  ],
  [
    "reassignHarness",
    () => reassignHarness("t1", "codex"),
    "thread|reassign|t1|--to|codex|",
    "Could not reassign harness",
  ],
  [
    "archiveThread",
    () => archiveThread("t1"),
    "thread|archive|t1|",
    "Could not archive thread",
  ],
  [
    "deleteThread",
    () => deleteThread("t1"),
    "thread|delete|t1|",
    "Could not delete thread",
  ],
  [
    "compactThread",
    () => compactThread("t1", CompactMode.Intelligent),
    "thread|compact|t1|--mode|intelligent|",
    "Could not compact thread",
  ],
  [
    "setMode",
    () => setMode("t1", ThreadMode.Plan),
    "thread|mode|t1|plan|",
    "Could not set mode",
  ],
  [
    "answerRequest",
    () => answerRequest("t1", "r1", { decision: Decision.Approve }),
    "thread|answer|t1|--request|r1|--approve|",
    "Could not answer request",
  ],
  [
    "noteThread",
    () => noteThread("t1", { note: "n" }),
    "thread|note|t1|--note|n|",
    "Could not note thread",
  ],
];

for (const [name, invoke, expected, fallback] of voidMutations) {
  test(`${name} invokes the core and reports failures`, async () => {
    await invoke();
    expect(await calls()).toEqual([expected]);

    await control("fail");
    await expect(invoke()).rejects.toThrow(fallback);
    await control("out", "from stdout");
    await expect(invoke()).rejects.toThrow("from stdout");
    await control("err", "from stderr");
    await expect(invoke()).rejects.toThrow("from stderr");
  });
}

test("answerRequest sends the decision and any text as flags", async () => {
  await answerRequest("t1", "r1", { decision: Decision.Deny, text: "too big" });
  await answerRequest("t1", "r2", { text: "blue" });
  await answerRequest("t1", "r3", { decision: Decision.Deny, text: "  " });
  expect(await calls()).toEqual([
    "thread|answer|t1|--request|r1|--deny|--text|too big|",
    "thread|answer|t1|--request|r2|--text|blue|",
    "thread|answer|t1|--request|r3|--deny|",
  ]);
});

test("noteThread forwards notes, decisions and questions", async () => {
  await noteThread("t1", {
    note: "n",
    decisions: ["d1", "d2"],
    questions: ["q"],
  });
  expect(await calls()).toEqual([
    "thread|note|t1|--note|n|--decision|d1|--decision|d2|--question|q|",
  ]);
});

test("createThread passes the harness and workspace root when set", async () => {
  delete process.env.MITOS_WORKSPACE_ROOT;
  await control("out", '{"id":"t9"}');
  expect(await createThread(null)).toEqual({ id: "t9" } as never);
  process.env.MITOS_WORKSPACE_ROOT = "/ws";
  await createThread("codex");
  expect(await calls()).toEqual([
    "thread|new|--json|",
    "thread|new|--harness|codex|--workspace|/ws|--json|",
  ]);
});

test("hooks commands pass harness filters and parse the result", async () => {
  await control("out", "[]");
  expect(await hooksStatus()).toEqual([]);
  expect(await hooksInit(["claude", "codex"])).toEqual([]);
  expect(await calls()).toEqual([
    "hooks|status|--json|",
    "hooks|init|--harness|claude|--harness|codex|--json|",
  ]);
});
