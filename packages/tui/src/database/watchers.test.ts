import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { watchDatabase, watchHistory } from "@database/watchers.ts";

let dir: string;

beforeEach(async () => {
  dir = join(tmpdir(), `mitos-watch-test-${crypto.randomUUID()}`);
  await mkdir(dir, { recursive: true });
});

afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("database file changes are reported once, debounced", async () => {
  let changes = 0;
  const stop = watchDatabase(join(dir, "mitos.db"), () => {
    changes += 1;
  });
  await writeFile(join(dir, "mitos.db-wal"), "a");
  await writeFile(join(dir, "mitos.db-wal"), "b");
  await sleep(300);
  stop();
  expect(changes).toBe(1);
});

test("unrelated files do not trigger a change", async () => {
  let changes = 0;
  const stop = watchDatabase(join(dir, "mitos.db"), () => {
    changes += 1;
  });
  await writeFile(join(dir, "other.txt"), "a");
  await sleep(200);
  stop();
  expect(changes).toBe(0);
});

test("stopping cancels a pending notification", async () => {
  let changes = 0;
  const stop = watchDatabase(join(dir, "mitos.db"), () => {
    changes += 1;
  });
  await writeFile(join(dir, "mitos.db"), "a");
  await sleep(10);
  stop();
  await sleep(200);
  expect(changes).toBe(0);
});

test("an unwatchable directory yields a harmless cleanup", () => {
  const stop = watchDatabase(join(dir, "missing", "mitos.db"), () => {});
  expect(stop).not.toThrow();
});

test("nested history files are reported once, debounced", async () => {
  await mkdir(join(dir, "projects"), { recursive: true });
  let changes = 0;
  const stop = watchHistory(join(dir, "projects"), () => {
    changes += 1;
  });
  const nested = join(dir, "projects", "-home-work");
  await mkdir(nested, { recursive: true });
  await writeFile(join(nested, "session.jsonl"), "a");
  await writeFile(join(nested, "session.jsonl"), "b");
  await sleep(300);
  stop();
  expect(changes).toBe(1);
});

test("an unwatchable history root yields a harmless cleanup", () => {
  const stop = watchHistory(join(dir, "missing"), () => {});
  expect(stop).not.toThrow();
});
