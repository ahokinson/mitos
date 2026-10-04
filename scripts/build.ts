import { mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { bundleSources } from "@builds/bundles.ts";
import { bin, dist, tui } from "@builds/layouts.ts";
import {
  installBinary,
  installRuntimeDependencies,
  writeLibraryPackage,
} from "@builds/packaging.ts";

await rm(dist, { recursive: true, force: true });
await mkdir(tui, { recursive: true });
await mkdir(bin, { recursive: true });

await bundleSources();
await writeLibraryPackage();
await installRuntimeDependencies();
const executable = await installBinary();

console.log(`Mitos release layout built at ${dist}`);
if (executable) console.log(`  ${executable}`);
console.log(`  ${join(tui, "main.mjs")}`);
