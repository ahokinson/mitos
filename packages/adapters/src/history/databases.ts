import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";

export function openReadonlyDatabase(path: string): Database | null {
  if (!existsSync(path)) return null;
  try {
    return new Database(path, { readonly: true });
  } catch {
    return null;
  }
}
