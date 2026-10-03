import { chmod, copyFile, cp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { bin, library, root, run } from "@builds/layouts.ts";

export async function writeLibraryPackage(): Promise<void> {
  const tuiPackage = JSON.parse(
    await readFile(join(root, "packages", "tui", "package.json"), "utf8"),
  ) as { name: string; version: string; dependencies: Record<string, string> };

  await writeFile(
    join(library, "package.json"),
    `${JSON.stringify(
      {
        name: tuiPackage.name,
        version: tuiPackage.version,
        private: true,
        type: "module",
        dependencies: {
          "@opentui/core": tuiPackage.dependencies["@opentui/core"],
        },
      },
      null,
      2,
    )}\n`,
  );
}

// MITOS_VENDORED_NODE_MODULES lets sandboxed builds (Nix) skip the registry.
export async function installRuntimeDependencies(): Promise<void> {
  const vendoredModules = process.env.MITOS_VENDORED_NODE_MODULES;
  if (vendoredModules) {
    await cp(vendoredModules, join(library, "node_modules"), {
      recursive: true,
    });
  } else {
    run(["bun", "install", "--production"], library);
  }
}

// MITOS_SKIP_CARGO lets a packager that builds the Rust binary itself assemble dist/bin.
export async function installBinary(): Promise<string | null> {
  if (process.env.MITOS_SKIP_CARGO) return null;
  run(["cargo", "build", "--release", "-p", "mitos"]);
  const executable = process.platform === "win32" ? "mitos.exe" : "mitos";
  const destination = join(bin, executable);
  await copyFile(join(root, "target", "release", executable), destination);
  if (process.platform !== "win32") await chmod(destination, 0o755);
  return destination;
}
