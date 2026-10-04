import type { ThreadEvent } from "@session/events.ts";

enum Sign {
  Context = " ",
  Added = "+",
  Removed = "-",
}

type DiffLine = { sign: Sign; text: string };

type Hunk = { oldStart: number; newStart: number; lines: DiffLine[] };

export type FileDiff = {
  path: string;
  diff: string;
  added: number;
  removed: number;
  truncated: number;
};

const CONTEXT_LINES = 3;
const MAX_DIFF_LINES = 60;
const MAX_LCS_CELLS = 4_000_000;

type Json = Record<string, unknown>;

function asRecord(value: unknown): Json | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Json)
    : null;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

function lineOps(oldLines: string[], newLines: string[]): DiffLine[] {
  let head = 0;
  while (
    head < oldLines.length &&
    head < newLines.length &&
    oldLines[head] === newLines[head]
  )
    head++;
  let tail = 0;
  while (
    tail < oldLines.length - head &&
    tail < newLines.length - head &&
    oldLines[oldLines.length - 1 - tail] ===
      newLines[newLines.length - 1 - tail]
  )
    tail++;

  const oldMid = oldLines.slice(head, oldLines.length - tail);
  const newMid = newLines.slice(head, newLines.length - tail);
  const ops: DiffLine[] = oldLines
    .slice(0, head)
    .map((text) => ({ sign: Sign.Context, text }));

  if (oldMid.length * newMid.length > MAX_LCS_CELLS) {
    for (const text of oldMid) ops.push({ sign: Sign.Removed, text });
    for (const text of newMid) ops.push({ sign: Sign.Added, text });
  } else {
    const width = newMid.length + 1;
    const table = new Uint32Array((oldMid.length + 1) * width);
    for (let i = oldMid.length - 1; i >= 0; i--) {
      for (let j = newMid.length - 1; j >= 0; j--) {
        table[i * width + j] =
          oldMid[i] === newMid[j]
            ? (table[(i + 1) * width + j + 1] ?? 0) + 1
            : Math.max(
                table[(i + 1) * width + j] ?? 0,
                table[i * width + j + 1] ?? 0,
              );
      }
    }
    let i = 0;
    let j = 0;
    while (i < oldMid.length && j < newMid.length) {
      if (oldMid[i] === newMid[j]) {
        ops.push({ sign: Sign.Context, text: oldMid[i] as string });
        i++;
        j++;
      } else if (
        (table[(i + 1) * width + j] ?? 0) >= (table[i * width + j + 1] ?? 0)
      ) {
        ops.push({ sign: Sign.Removed, text: oldMid[i] as string });
        i++;
      } else {
        ops.push({ sign: Sign.Added, text: newMid[j] as string });
        j++;
      }
    }
    for (; i < oldMid.length; i++)
      ops.push({ sign: Sign.Removed, text: oldMid[i] as string });
    for (; j < newMid.length; j++)
      ops.push({ sign: Sign.Added, text: newMid[j] as string });
  }

  for (const text of oldLines.slice(oldLines.length - tail))
    ops.push({ sign: Sign.Context, text });
  return ops;
}

function groupHunks(ops: DiffLine[]): Hunk[] {
  const changed: number[] = [];
  ops.forEach((op, index) => {
    if (op.sign !== Sign.Context) changed.push(index);
  });
  if (changed.length === 0) return [];

  const ranges: Array<[number, number]> = [];
  for (const index of changed) {
    const start = Math.max(index - CONTEXT_LINES, 0);
    const end = Math.min(index + CONTEXT_LINES, ops.length - 1);
    const last = ranges[ranges.length - 1];
    if (last && start <= last[1] + 1) last[1] = Math.max(last[1], end);
    else ranges.push([start, end]);
  }

  return ranges.map(([start, end]) => {
    let oldStart = 1;
    let newStart = 1;
    for (const op of ops.slice(0, start)) {
      if (op.sign !== Sign.Added) oldStart++;
      if (op.sign !== Sign.Removed) newStart++;
    }
    const lines = ops.slice(start, end + 1);
    const isBlankContext = (line: DiffLine | undefined) =>
      line?.sign === Sign.Context && line.text.trim() === "";
    while (isBlankContext(lines[0])) {
      lines.shift();
      oldStart++;
      newStart++;
    }
    while (isBlankContext(lines[lines.length - 1])) lines.pop();
    return { oldStart, newStart, lines };
  });
}

function countOf(lines: DiffLine[], sign: Sign): number {
  return lines.filter((line) => line.sign === sign).length;
}

function capHunks(
  hunks: Hunk[],
  maxLines: number,
): { hunks: Hunk[]; truncated: number } {
  const total = hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0);
  if (total <= maxLines) return { hunks, truncated: 0 };
  const kept: Hunk[] = [];
  let remaining = maxLines;
  for (const hunk of hunks) {
    if (remaining <= 0) break;
    kept.push({ ...hunk, lines: hunk.lines.slice(0, remaining) });
    remaining -= hunk.lines.length;
  }
  return { hunks: kept, truncated: total - maxLines };
}

function formatDiff(path: string, hunks: Hunk[]): string {
  const out = [`--- a/${path}`, `+++ b/${path}`];
  for (const hunk of hunks) {
    const oldCount = hunk.lines.length - countOf(hunk.lines, Sign.Added);
    const newCount = hunk.lines.length - countOf(hunk.lines, Sign.Removed);
    out.push(
      `@@ -${hunk.oldStart},${oldCount} +${hunk.newStart},${newCount} @@`,
    );
    for (const line of hunk.lines) out.push(`${line.sign}${line.text}`);
  }
  return `${out.join("\n")}\n`;
}

function fileDiffOf(path: string, hunks: Hunk[]): FileDiff | null {
  if (hunks.length === 0) return null;
  const added = hunks.reduce((n, h) => n + countOf(h.lines, Sign.Added), 0);
  const removed = hunks.reduce((n, h) => n + countOf(h.lines, Sign.Removed), 0);
  const capped = capHunks(hunks, MAX_DIFF_LINES);
  return {
    path,
    diff: formatDiff(path, capped.hunks),
    added,
    removed,
    truncated: capped.truncated,
  };
}

/** Unified diff of two texts; `null` when they have no differing lines. */
export function diffTexts(
  path: string,
  oldText: string,
  newText: string,
): FileDiff | null {
  return fileDiffOf(
    path,
    groupHunks(lineOps(splitLines(oldText), splitLines(newText))),
  );
}

function parseUnified(body: string): Hunk[] {
  const hunks: Hunk[] = [];
  let current: Hunk | null = null;
  for (const raw of body.split("\n")) {
    const header = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
    if (header) {
      current = {
        oldStart: Number(header[1]),
        newStart: Number(header[2]),
        lines: [],
      };
      hunks.push(current);
      continue;
    }
    if (!current) continue;
    const sign = raw[0];
    if (sign === Sign.Added || sign === Sign.Removed || sign === Sign.Context)
      current.lines.push({ sign, text: raw.slice(1) });
  }
  return hunks.filter((hunk) => hunk.lines.length > 0);
}

function diffFromUnified(path: string, body: string): FileDiff | null {
  return fileDiffOf(path, parseUnified(body));
}

function claudeResultDiffs(payload: Json): FileDiff[] {
  const result = asRecord(payload.tool_use_result);
  const path = asString(result?.filePath);
  if (!result || !path) return [];
  const patch = Array.isArray(result.structuredPatch)
    ? result.structuredPatch
    : [];
  const hunks = patch.flatMap((entry): Hunk[] => {
    const record = asRecord(entry);
    if (!record || !Array.isArray(record.lines)) return [];
    const lines = record.lines.flatMap((raw): DiffLine[] => {
      if (typeof raw !== "string") return [];
      const sign = raw[0];
      return sign === Sign.Added ||
        sign === Sign.Removed ||
        sign === Sign.Context
        ? [{ sign, text: raw.slice(1) }]
        : [];
    });
    return lines.length === 0
      ? []
      : [
          {
            oldStart: Number(record.oldStart) || 1,
            newStart: Number(record.newStart) || 1,
            lines,
          },
        ];
  });
  const fromPatch = fileDiffOf(path, hunks);
  if (fromPatch) return [fromPatch];
  const created = asString(result.content);
  const diff =
    result.type === "create" && created !== undefined
      ? diffTexts(path, "", created)
      : diffTexts(
          path,
          asString(result.oldString) ?? "",
          asString(result.newString) ?? "",
        );
  return diff ? [diff] : [];
}

function codexDiffs(payload: Json): FileDiff[] {
  const changes = Array.isArray(payload.changes) ? payload.changes : [];
  return changes.flatMap((change) => {
    const record = asRecord(change);
    const path = asString(record?.path);
    const body = asString(record?.diff);
    if (!record || !path || body === undefined) return [];
    const kind = asString(asRecord(record.kind)?.type) ?? asString(record.kind);
    const diff =
      body.includes("\n@@") || body.startsWith("@@")
        ? diffFromUnified(path, body)
        : kind === "delete"
          ? diffTexts(path, body, "")
          : diffTexts(path, "", body);
    return diff ? [diff] : [];
  });
}

function opencodeDiffs(payload: Json): FileDiff[] {
  const state = asRecord(payload.state);
  const input = asRecord(state?.input);
  const path = asString(input?.filePath) ?? asString(input?.path);
  if (!state || !input || !path) return [];
  const unified = asString(asRecord(state.metadata)?.diff);
  if (unified) {
    const diff = diffFromUnified(path, unified);
    if (diff) return [diff];
  }
  const diff =
    payload.tool === "write"
      ? diffTexts(path, "", asString(input.content) ?? "")
      : payload.tool === "edit"
        ? diffTexts(
            path,
            asString(input.oldString) ?? "",
            asString(input.newString) ?? "",
          )
        : null;
  return diff ? [diff] : [];
}

function hermesDiffs(payload: Json): FileDiff[] {
  const blocks = Array.isArray(payload.content) ? payload.content : [];
  return blocks.flatMap((block) => {
    const record = asRecord(block);
    const path = asString(record?.path);
    if (record?.type !== "diff" || !path) return [];
    const diff = diffTexts(
      path,
      asString(record.oldText) ?? "",
      asString(record.newText) ?? "",
    );
    return diff ? [diff] : [];
  });
}

/** File diffs carried by a tool-call payload, across the four harnesses'
 * shapes. Empty for anything that isn't a file edit. */
export function payloadDiffs(payload: unknown): FileDiff[] {
  const record = asRecord(payload);
  if (!record) return [];
  if (record.type === "tool_use") return [];
  if (record.type === "tool_result") return claudeResultDiffs(record);
  if (record.type === "fileChange") return codexDiffs(record);
  if (record.type === "tool") return opencodeDiffs(record);
  return hermesDiffs(record);
}

export function eventDiffs(event: ThreadEvent): FileDiff[] {
  return payloadDiffs(event.payload);
}
