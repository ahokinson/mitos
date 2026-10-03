import { stringValue } from "@json/scalars.ts";

export type JsonRecord = Record<string, unknown>;

export function asRecord(value: unknown): JsonRecord | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as JsonRecord)
    : null;
}

export function isRecord(value: unknown): value is JsonRecord {
  return asRecord(value) !== null;
}

/** Unlike `isRecord`, accepts arrays. */
export function isObject(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null;
}

export function parsedRecord(value: unknown): JsonRecord | null {
  if (isObject(value)) return value;
  if (typeof value !== "string") return null;
  try {
    const parsed = JSON.parse(value) as unknown;
    return isObject(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function findString(value: unknown, keys: string[]): string | null {
  if (!isObject(value)) return null;
  for (const key of keys) {
    const direct = stringValue(value[key]);
    if (direct) return direct;
  }
  for (const nested of Object.values(value)) {
    const found = findString(nested, keys);
    if (found) return found;
  }
  return null;
}
