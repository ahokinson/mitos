import { cp } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { root, tui } from "@builds/layouts.ts";
import { createSolidTransformPlugin } from "@opentui/solid/bun-plugin";

const PARSER_FILETYPES = ["bash", "json", "rust", "toml"];

function installedParserWasmRoot(): string {
  const tuiRequire = createRequire(
    join(root, "packages", "tui", "package.json"),
  );
  return join(dirname(tuiRequire.resolve("tree-sitter-wasm")), "out");
}

export async function bundleSources(): Promise<void> {
  const tuiBundle = await Bun.build({
    entrypoints: [join(root, "packages", "tui", "src", "app", "entries.tsx")],
    outdir: tui,
    naming: "main.mjs",
    target: "bun",
    // @opentui/core ships platform-native optional dependencies Bun.build
    // can't bundle, so it stays external and is installed separately.
    external: ["@opentui/core"],
    plugins: [createSolidTransformPlugin()],
  });
  await cp(join(root, "packages", "tui", "assets"), join(tui, "assets"), {
    recursive: true,
  });
  const wasmRoot = installedParserWasmRoot();
  for (const filetype of PARSER_FILETYPES) {
    await cp(
      join(wasmRoot, filetype, `tree-sitter-${filetype}.wasm`),
      join(tui, "assets", "parsers", filetype, `tree-sitter-${filetype}.wasm`),
    );
  }
  if (!tuiBundle.success) {
    for (const log of tuiBundle.logs) console.error(log);
    process.exit(1);
  }
}
