import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { addDefaultParsers } from "@opentui/core";

const LANGUAGES = [
  { filetype: "rust", aliases: ["rs"] },
  { filetype: "bash", aliases: ["sh", "shell", "zsh"] },
  { filetype: "json", aliases: ["jsonc"] },
  { filetype: "toml", aliases: [] },
];

// From source the assets sit at the package root; the release bundle ships
// them next to `main.mjs`.
const ROOTS = ["../../assets/parsers", "./assets/parsers"].map((path) =>
  fileURLToPath(new URL(path, import.meta.url)),
);

/** Tree-sitter grammars shipped with the app, beyond the ones OpenTui bundles. */
export function registerParsers(): void {
  const root = ROOTS.find((path) => existsSync(path));
  if (root === undefined) return;
  addDefaultParsers(
    LANGUAGES.map(({ filetype, aliases }) => ({
      filetype,
      aliases,
      queries: { highlights: [`${root}/${filetype}/highlights.scm`] },
      wasm: `${root}/${filetype}/tree-sitter-${filetype}.wasm`,
    })),
  );
}
