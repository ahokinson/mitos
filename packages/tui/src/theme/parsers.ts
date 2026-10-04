import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { addDefaultParsers } from "@opentui/core";

const LANGUAGES = [
  { filetype: "rust", aliases: ["rs"] },
  { filetype: "bash", aliases: ["sh", "shell", "zsh"] },
  { filetype: "json", aliases: ["jsonc"] },
  { filetype: "toml", aliases: [] },
];

// From source the queries sit at the package root; the release bundle ships
// them next to `main.mjs` with the four parser binaries.
const ASSET_ROOTS = ["../../assets/parsers", "./assets/parsers"].map((path) =>
  fileURLToPath(new URL(path, import.meta.url)),
);

function installedWasmRoot(): string | undefined {
  try {
    return fileURLToPath(
      new URL("./out/", import.meta.resolve("tree-sitter-wasm")),
    );
  } catch {
    return undefined;
  }
}

function hasWasm(root: string): boolean {
  return LANGUAGES.every(({ filetype }) =>
    existsSync(join(root, filetype, `tree-sitter-${filetype}.wasm`)),
  );
}

/** Tree-sitter grammars shipped with the app, beyond the ones OpenTui bundles. */
export function registerParsers(): void {
  const queryRoot = ASSET_ROOTS.find((path) => existsSync(path));
  const wasmRoot = ASSET_ROOTS.find(hasWasm) ?? installedWasmRoot();
  if (queryRoot === undefined || wasmRoot === undefined) return;
  addDefaultParsers(
    LANGUAGES.map(({ filetype, aliases }) => ({
      filetype,
      aliases,
      queries: { highlights: [join(queryRoot, filetype, "highlights.scm")] },
      wasm: join(wasmRoot, filetype, `tree-sitter-${filetype}.wasm`),
    })),
  );
}
