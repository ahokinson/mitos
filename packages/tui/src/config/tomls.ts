export function warn(message: string): void {
  console.error(`mitos: ${message}`);
}

export async function importToml(path: string): Promise<unknown> {
  if (!path.endsWith(".toml")) throw new Error("not a .toml path");
  const mod = (await import(path)) as { default?: unknown };
  return mod.default ?? mod;
}

export function table(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}
