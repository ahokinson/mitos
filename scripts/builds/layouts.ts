import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("../..", import.meta.url));
export const dist = join(root, "dist");
export const bin = join(dist, "bin");
export const library = join(dist, "lib", "mitos");
export const tui = join(library, "tui");
export const adapters = join(library, "adapters");
export const harnesses = ["claude", "codex", "hermes", "opencode"];

export function run(command: string[], cwd = root) {
  const result = Bun.spawnSync(command, {
    cwd,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  if (result.exitCode !== 0) process.exit(result.exitCode);
}
