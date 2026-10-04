import { cp } from "node:fs/promises";
import { join } from "node:path";
import { root, tui } from "@builds/layouts.ts";
import { createSolidTransformPlugin } from "@opentui/solid/bun-plugin";

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
  if (!tuiBundle.success) {
    for (const log of tuiBundle.logs) console.error(log);
    process.exit(1);
  }
}
