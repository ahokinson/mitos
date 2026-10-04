import { afterEach, beforeEach, expect, spyOn, test } from "bun:test"
import { testRender } from "@opentui/solid"
import { chmod, mkdir, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"

import { App } from "@app/shells.tsx"
import { loadConfig } from "@config/loaders.ts"
import { resolveTheme } from "@theme/palettes.ts"
import { ThemeProvider } from "@theme/providers.tsx"

/** `detectInstalledHarnesses` is a real PATH lookup; pinned through
 * `Bun.which` so the shell's "is the default harness usable" branches don't
 * depend on the machine. */
let installed: string[] = ["claude", "codex"]
let whichSpy: ReturnType<typeof spyOn>

const THREAD = {
  id: "launch-1",
  workspace_id: "ws",
  status: "active",
  mode: "build",
  active_harness: "claude",
  native_session: null,
  last_event_seq: 0,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  opening_message: null,
}

const FAKE_CORE = `#!/bin/sh
here=$(dirname "$0")
line=$(printf '%s|' "$@")
echo "$line" >> "$here/calls.log"
case "$1 $2" in
  "thread new")
    if [ -f "$here/new-fail" ]; then echo "cannot create" >&2; exit 1; fi
    cat "$here/thread.json" ;;
  "view threads") cat "$here/threads.json" ;;
  "thread sync")
    if [ -f "$here/events.json" ]; then cat "$here/events.json"; mv "$here/events.json" "$here/events.done"; else echo '[]'; fi ;;
  "thread send")
    if [ -f "$here/send-fail" ]; then echo "send failed" >&2; exit 1; fi ;;
  "view usage") echo null ;;
  "thread requests") echo '[]' ;;
  "view history") echo '["earlier one"]' ;;
  "view empty") echo false ;;
esac
exit 0
`

let dir: string
const savedEnv: Record<string, string | undefined> = {}
let exitSpy: ReturnType<typeof spyOn>
let errorSpy: ReturnType<typeof spyOn>

function setEnv(name: string, value: string): void {
  if (!(name in savedEnv)) savedEnv[name] = process.env[name]
  process.env[name] = value
}

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-shell-test-${crypto.randomUUID()}`)
  await mkdir(dir, { recursive: true })
  installed = ["claude", "codex"]
  const script = join(dir, "core.sh")
  await writeFile(script, FAKE_CORE)
  await chmod(script, 0o755)
  await writeFile(join(dir, "thread.json"), JSON.stringify(THREAD))
  await writeFile(join(dir, "threads.json"), JSON.stringify([THREAD]))
  await writeFile(join(dir, "mitos.toml"), '[session]\ndefault_harness = "claude"\n')
  setEnv("MITOS_CORE", script)
  setEnv("MITOS_STATE_DIR", dir)
  setEnv("MITOS_WORKSPACE_KEY", "ws-key")
  exitSpy = spyOn(process, "exit").mockImplementation((() => undefined) as never)
  errorSpy = spyOn(console, "error").mockImplementation(() => {})
  whichSpy = spyOn(Bun, "which").mockImplementation(((binary: string) =>
    installed.includes(binary) ? `/usr/bin/${binary}` : null) as never)
})

afterEach(async () => {
  whichSpy.mockRestore()
  exitSpy.mockRestore()
  errorSpy.mockRestore()
  for (const [name, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[name]
    else process.env[name] = value
  }
  await rm(dir, { recursive: true, force: true })
})

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

async function mount(config?: string) {
  if (config !== undefined) await writeFile(join(dir, "mitos.toml"), config)
  const handle = await loadConfig(dir)
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <App configHandle={handle} />
      </ThemeProvider>
    ),
    { width: 100, height: 40 },
  )
  async function settle(): Promise<string> {
    await sleep(250)
    await setup.renderOnce()
    return setup.captureCharFrame()
  }
  await settle()
  return { setup, settle }
}

async function calls(): Promise<string[]> {
  return (await readFile(join(dir, "calls.log"), "utf8")).trim().split("\n")
}

test("launching starts a thread on the configured harness and shows the composer", async () => {
  const { setup, settle } = await mount()
  try {
    const frame = await settle()
    expect(frame).toContain("› Message")
    expect(frame).not.toContain("Start a conversation")
    expect(await calls()).toContain("thread|new|--harness|claude|--json|")
  } finally {
    setup.renderer.destroy()
  }
})

test("a typed message is sent to the launched thread and its events appear", async () => {
  await writeFile(
    join(dir, "events.json"),
    JSON.stringify([
      {
        thread_id: "launch-1",
        seq: 1,
        turn_id: null,
        harness: "claude",
        kind: "user_message",
        role: "user",
        content: "hello from the log",
        payload: null,
        created_at: "2026-01-01T00:00:00Z",
      },
    ]),
  )
  const { setup, settle } = await mount()
  try {
    await setup.mockInput.typeText("hello")
    setup.mockInput.pressEnter()
    const frame = await settle()
    expect(await calls()).toContain("thread|send|launch-1|--message|hello|")
    expect(frame).toContain("hello from the log")
  } finally {
    setup.renderer.destroy()
  }
})

test("consecutive tool rows group and a folded edit hides its read", async () => {
  const base = {
    thread_id: "launch-1",
    turn_id: "turn-1",
    harness: "claude",
    role: null,
    created_at: "2026-01-01T00:00:00Z",
  }
  const tool = (overrides: object) => ({ kind: "other", name: null, argument: "", tool_use_id: null, path: null, shape: "other", quiet: false, patched: false, ...overrides })
  const events = [
    { ...base, seq: 1, kind: "tool_call", content: "Read", payload: { type: "tool_use", id: "r1", name: "Read", input: { file_path: "a.ts" } }, tool: tool({ kind: "read", name: "Read", argument: "a.ts", tool_use_id: "r1", path: "a.ts", shape: "tool_use" }) },
    { ...base, seq: 2, kind: "tool_call", content: "Edit", payload: { type: "tool_use", id: "e1", name: "Edit", input: { file_path: "a.ts", old_string: "oldValue", new_string: "newValue" } }, tool: tool({ kind: "edit", name: "Edit", argument: "a.ts", tool_use_id: "e1", path: "a.ts", shape: "tool_use" }) },
    {
      ...base,
      seq: 3,
      kind: "tool_result",
      content: "updated",
      payload: {
        type: "tool_result",
        tool_use_id: "e1",
        tool_use_result: { filePath: "a.ts", structuredPatch: [{ oldStart: 1, newStart: 1, lines: ["-oldValue", "+newValue"] }] },
      },
      diffs: [{ path: "a.ts", diff: "--- a/a.ts\n+++ b/a.ts\n@@ -1,1 +1,1 @@\n-oldValue\n+newValue\n", added: 1, removed: 1, truncated: 0 }],
      tool: tool({ shape: "tool_result", tool_use_id: "e1", quiet: true, patched: true }),
    },
    { ...base, seq: 4, kind: "tool_call", content: "Bash", payload: { type: "tool_use", id: "b1", name: "Bash", input: { command: "ls" } }, tool: tool({ kind: "shell", name: "Bash", argument: "ls", tool_use_id: "b1", shape: "tool_use" }) },
    { ...base, seq: 5, kind: "tool_result", content: "listing", payload: { type: "tool_result", tool_use_id: "b1" }, tool: tool({ shape: "tool_result", tool_use_id: "b1" }) },
  ]
  await writeFile(join(dir, "events.json"), JSON.stringify(events))
  const { setup, settle } = await mount()
  try {
    await setup.mockInput.typeText("go")
    setup.mockInput.pressEnter()
    const frame = await settle()
    expect(frame).toContain("newValue")
    expect(frame).toContain("$ ls")
    expect(frame).toContain("listing")
    expect(frame).not.toContain("≡ a.ts")
  } finally {
    setup.renderer.destroy()
  }
})

test("a failed send is shown as an error", async () => {
  await writeFile(join(dir, "send-fail"), "")
  const { setup, settle } = await mount()
  try {
    await setup.mockInput.typeText("hello")
    setup.mockInput.pressEnter()
    expect(await settle()).toContain("send failed")
  } finally {
    setup.renderer.destroy()
  }
})

test("slash commands run locally and print their output", async () => {
  const { setup, settle } = await mount()
  try {
    await setup.mockInput.typeText("/help")
    setup.mockInput.pressEnter()
    const frame = await settle()
    expect(frame).toContain("/archive")
    expect((await calls()).some((line) => line.startsWith("thread|send"))).toBe(false)
  } finally {
    setup.renderer.destroy()
  }
})

test("when launch cannot create a thread the error shows, and a message retries the default harness", async () => {
  await writeFile(join(dir, "new-fail"), "")
  const { setup, settle } = await mount()
  try {
    expect(await settle()).toContain("cannot create")
    await setup.mockInput.typeText("hello")
    setup.mockInput.pressEnter()
    await settle()
    const creations = (await calls()).filter((line) => line.startsWith("thread|new"))
    expect(creations).toHaveLength(2)
  } finally {
    setup.renderer.destroy()
  }
})

test("without a thread or an installed harness a message explains what to do", async () => {
  installed = []
  await writeFile(join(dir, "new-fail"), "")
  const { setup, settle } = await mount("")
  try {
    await setup.mockInput.typeText("hello")
    setup.mockInput.pressEnter()
    expect(await settle()).toContain("No harness selected")
  } finally {
    setup.renderer.destroy()
  }
})

test("a new draft says which harness the next message uses", async () => {
  const { setup, settle } = await mount()
  try {
    setup.mockInput.pressKey("n", { ctrl: true })
    expect(await settle()).toContain("next message creates a claude thread")
  } finally {
    setup.renderer.destroy()
  }
})

test("a new draft with no harness available says so", async () => {
  installed = []
  const { setup, settle } = await mount("")
  try {
    setup.mockInput.pressKey("n", { ctrl: true })
    expect(await settle()).toContain("no harness chosen")
  } finally {
    setup.renderer.destroy()
  }
})

test("blank input is ignored", async () => {
  const { setup, settle } = await mount()
  try {
    await setup.mockInput.typeText("   ")
    setup.mockInput.pressEnter()
    await settle()
    expect((await calls()).some((line) => line.startsWith("thread|send"))).toBe(false)
  } finally {
    setup.renderer.destroy()
  }
})

test("database changes refresh the thread list", async () => {
  const { setup, settle } = await mount()
  try {
    const before = (await calls()).filter((line) => line.startsWith("view|threads")).length
    await writeFile(join(dir, "mitos.db-wal"), "x")
    await settle()
    const after = (await calls()).filter((line) => line.startsWith("view|threads")).length
    expect(after).toBeGreaterThan(before)
  } finally {
    setup.renderer.destroy()
  }
})

test("ctrl+l refreshes", async () => {
  const { setup, settle } = await mount()
  try {
    const before = (await calls()).filter((line) => line.startsWith("view|threads")).length
    setup.mockInput.pressKey("l", { ctrl: true })
    await settle()
    const after = (await calls()).filter((line) => line.startsWith("view|threads")).length
    expect(after).toBeGreaterThan(before)
  } finally {
    setup.renderer.destroy()
  }
})

test("up arrow recalls an earlier message from this workspace's history", async () => {
  const { setup, settle } = await mount()
  try {
    setup.mockInput.pressArrow("up")
    expect(await settle()).toContain("earlier one")
    expect((await calls()).some((line) => line.startsWith("view|history|--workspace-key|ws-key|"))).toBe(true)
  } finally {
    setup.renderer.destroy()
  }
})

test("termination signals quit like ctrl+c", async () => {
  const { setup, settle } = await mount()
  try {
    process.emit("SIGHUP")
    await settle()
    expect(exitSpy).toHaveBeenCalledWith(0)
  } finally {
    setup.renderer.destroy()
  }
})

test("quitting prunes empty launch threads and exits once", async () => {
  const { setup, settle } = await mount()
  try {
    setup.mockInput.pressKey("c", { ctrl: true })
    setup.mockInput.pressKey("c", { ctrl: true })
    await settle()
    expect(exitSpy).toHaveBeenCalledTimes(1)
    expect(exitSpy).toHaveBeenCalledWith(0)
    expect((await calls()).some((line) => line.startsWith("view|empty|launch-1"))).toBe(true)
  } finally {
    setup.renderer.destroy()
  }
})
