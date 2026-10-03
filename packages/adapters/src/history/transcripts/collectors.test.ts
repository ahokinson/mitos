import { expect, test } from "bun:test";
import { mkdir, rm, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { collectJsonlHandoff } from "@history/transcripts/collectors.ts";
import { Action } from "@protocol/actions.ts";

test("selects a recent transcript even when older files fill the candidate budget", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const oldTimestamp = new Date(Date.now() - 3_600_000);
    const oldRecord = JSON.stringify({
      cwd: "/workspace",
      role: "user",
      content: "old",
    });
    for (let index = 0; index < 192; index += 1) {
      const path = join(root, `${String(index).padStart(3, "0")}.jsonl`);
      await writeFile(path, oldRecord);
      await utimes(path, oldTimestamp, oldTimestamp);
    }
    await writeFile(
      join(root, "recent.jsonl"),
      JSON.stringify({
        cwd: "/workspace",
        role: "assistant",
        content: "recent handoff evidence",
      }),
    );

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    expect(response.transcript?.messages).toEqual([
      { role: "assistant", text: "recent handoff evidence" },
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("drops the injected handoff preamble from mined messages", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const records = [
      JSON.stringify({ cwd: "/workspace" }),
      JSON.stringify({
        role: "user",
        content: "You are joining Mitos session t1.\n\n- user: hi",
      }),
      JSON.stringify({ role: "assistant", content: "hello" }),
    ];
    await writeFile(join(root, "recent.jsonl"), records.join("\n"));

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    expect(response.transcript?.messages).toEqual([
      { role: "assistant", text: "hello" },
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("keeps every mined message for the handoff layer to bound", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const records = [
      JSON.stringify({ cwd: "/workspace" }),
      ...Array.from({ length: 13 }, (_, index) =>
        JSON.stringify({
          role: index % 2 === 0 ? "user" : "assistant",
          content: `message ${index}`,
        }),
      ),
    ];
    await writeFile(join(root, "recent.jsonl"), records.join("\n"));

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    expect(response.transcript?.messages).toHaveLength(13);
    expect(response.transcript?.truncated).toBe(false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("records without a message id fall back to the last reported usage", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const records = [
      JSON.stringify({ cwd: "/workspace" }),
      JSON.stringify({
        role: "user",
        content: "first",
      }),
      JSON.stringify({
        role: "assistant",
        content: "first reply",
        message: {
          model: "claude-opus-5",
          usage: { input_tokens: 100, output_tokens: 50 },
        },
      }),
      JSON.stringify({
        role: "user",
        content: "second",
      }),
      JSON.stringify({
        role: "assistant",
        content: "second reply",
        message: {
          model: "claude-opus-5",
          usage: {
            input_tokens: 900,
            output_tokens: 340,
            cache_read_input_tokens: 12_000,
          },
        },
      }),
    ];
    await writeFile(join(root, "recent.jsonl"), records.join("\n"));

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    expect(response.usage).toEqual({
      input_tokens: 900,
      output_tokens: 340,
      cached_input_tokens: 12_000,
      model: "claude-opus-5",
      turns: 2,
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("claude token usage sums each distinct message once", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const assistant = (
      id: string,
      usage: Record<string, number>,
      content: string,
    ) =>
      JSON.stringify({
        role: "assistant",
        content,
        message: { id, model: "claude-opus-5", usage },
      });
    const records = [
      JSON.stringify({ cwd: "/workspace" }),
      JSON.stringify({ role: "user", content: "go" }),
      assistant("msg_1", { input_tokens: 100, output_tokens: 10 }, "part a"),
      assistant("msg_1", { input_tokens: 100, output_tokens: 10 }, "part b"),
      assistant(
        "msg_2",
        { input_tokens: 200, output_tokens: 30, cache_read_input_tokens: 500 },
        "done",
      ),
    ];
    await writeFile(join(root, "recent.jsonl"), records.join("\n"));

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    expect(response.usage).toMatchObject({
      input_tokens: 300,
      output_tokens: 40,
      cached_input_tokens: 500,
      model: "claude-opus-5",
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("usage is absent (not a guessed zero) when no record carries it", async () => {
  const root = join(tmpdir(), `mitos-jsonl-test-${crypto.randomUUID()}`);
  await mkdir(root, { recursive: true });
  try {
    const records = [
      JSON.stringify({ cwd: "/workspace" }),
      JSON.stringify({ role: "user", content: "hi" }),
      JSON.stringify({ role: "assistant", content: "hello" }),
    ];
    await writeFile(join(root, "recent.jsonl"), records.join("\n"));

    const response = await collectJsonlHandoff(
      {
        protocol_version: 1,
        action: Action.CollectHandoff,
        harness: "test",
        workdir: "/workspace",
        launched_at: new Date().toISOString(),
        native_session: null,
      },
      "test transcript",
      root,
    );

    // Still worth reporting a turn count even without token/model data.
    expect(response.usage).toEqual({ turns: 1 });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
