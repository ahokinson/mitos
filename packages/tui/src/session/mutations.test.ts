import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  abortThread,
  answerRequest,
  archiveThread,
  compactThread,
  createThread,
  deleteThread,
  diffThread,
  forkThread,
  hooksInit,
  hooksStatus,
  listAgents,
  listModels,
  noteThread,
  reassignHarness,
  runCommand,
  sendMessage,
  setAgent,
  setMode,
  setModel,
  shareThread,
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
    "abortThread",
    () => abortThread("t1"),
    "thread|abort|t1|",
    "Could not abort turn",
  ],
  [
    "setAgent",
    () => setAgent("t1", "build"),
    "thread|agent|t1|build|",
    "Could not set agent",
  ],
  [
    "setModel",
    () => setModel("t1", "opencode/glm-5.3"),
    "thread|model|t1|opencode/glm-5.3|",
    "Could not set model",
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

/** Mutations whose payload is the core's stdout: a URL, summary lines, or
 * trimmed text. */
const stdoutMutations: [
  string,
  () => Promise<string | string[]>,
  string,
  string,
  string,
  string | string[],
][] = [
  [
    "shareThread",
    () => shareThread("t1"),
    "thread|share|t1|",
    "Could not share thread",
    "https://opencode.ai/s/1",
    "https://opencode.ai/s/1",
  ],
  [
    "diffThread",
    () => diffThread("t1"),
    "thread|diff|t1|",
    "Could not read diff",
    "src/a.rs: +3 -1\n\n  src/b.rs  \n\n",
    ["src/a.rs: +3 -1", "src/b.rs"],
  ],
  [
    "listAgents",
    () => listAgents("t1"),
    "thread|agents|t1|",
    "Could not list agents",
    "build: writes code\nplan: read only",
    ["build: writes code", "plan: read only"],
  ],
  [
    "listModels",
    () => listModels("t1"),
    "thread|models|t1|",
    "Could not list models",
    "opencode/glm-5.3",
    ["opencode/glm-5.3"],
  ],
  [
    "runCommand",
    () => runCommand("t1", "init", ["--force"]),
    "thread|command|t1|init|--force|",
    "Could not run the command",
    "done",
    "done",
  ],
];

for (const [
  name,
  invoke,
  expected,
  fallback,
  out,
  returns,
] of stdoutMutations) {
  test(`${name} invokes the core, returns its stdout and reports failures`, async () => {
    await control("out", out);
    expect(await invoke()).toEqual(returns);
    expect(await calls()).toEqual([expected]);

    await control("out");
    await control("fail");
    await expect(invoke()).rejects.toThrow(fallback);
    await control("out", "from stdout");
    await expect(invoke()).rejects.toThrow("from stdout");
    await control("err", "from stderr");
    await expect(invoke()).rejects.toThrow("from stderr");
  });
}

test("forkThread parses the forked thread and reports failures", async () => {
  await control("out", '{"id":"t2"}');
  expect(await forkThread("t1")).toEqual({ id: "t2" } as never);
  expect(await calls()).toEqual(["thread|fork|t1|--json|"]);

  await control("out");
  await control("fail");
  await expect(forkThread("t1")).rejects.toThrow("Mitos command failed");
});

test("setAgent and setModel omit the value when it is null", async () => {
  await setAgent("t1", "build");
  await setAgent("t1", null);
  await setModel("t1", "opencode/glm-5.3");
  await setModel("t1", null);
  expect(await calls()).toEqual([
    "thread|agent|t1|build|",
    "thread|agent|t1|",
    "thread|model|t1|opencode/glm-5.3|",
    "thread|model|t1|",
  ]);
});

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
