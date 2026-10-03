import { homedir } from "node:os";
import { dirname, isAbsolute, join } from "node:path";

/** Mirrors `crates/mitos/src/store/mod.rs::resolve_root`'s fallback chain —
 * Rust core's `mitos.db` and the TUI's own `mitos.toml` deliberately share
 * this one directory. An explicit override is used verbatim; otherwise
 * XDG_CONFIG_HOME if set and non-empty, else ~/.config, with "mitos"
 * appended in both fallback arms. */
export function resolveConfigDir(overrideDir?: string): string {
  if (overrideDir) return overrideDir;
  const xdgConfigHome = process.env.XDG_CONFIG_HOME;
  const base =
    xdgConfigHome && xdgConfigHome.length > 0
      ? xdgConfigHome
      : join(homedir(), ".config");
  return join(base, "mitos");
}

export function resolveConfigPath(overrideDir?: string): string {
  return join(resolveConfigDir(overrideDir), "mitos.toml");
}

/** Where Rust core's `Store::resolve_root` puts `mitos.db`, in the same
 * directory as `mitos.toml` — see `resolveConfigDir`. */
export function resolveDbPath(overrideDir?: string): string {
  return join(resolveConfigDir(overrideDir), "mitos.db");
}

/** Expands a leading `~` to the home directory; otherwise returned as-is. */
export function expandHome(path: string): string {
  if (path === "~") return homedir();
  if (path.startsWith("~/")) return join(homedir(), path.slice(2));
  return path;
}

/** Resolves a `[themes]` path entry: `~`-expanded, then relative paths are
 * taken relative to the directory mitos.toml itself lives in (so a theme
 * file can ship alongside mitos.toml in a dotfiles repo). */
function resolveConfigRelativePath(
  rawPath: string,
  configPath: string,
): string {
  const expanded = expandHome(rawPath);
  return isAbsolute(expanded) ? expanded : join(dirname(configPath), expanded);
}

export function resolveThemePath(rawPath: string, configPath: string): string {
  return resolveConfigRelativePath(rawPath, configPath);
}

/** Resolves a template view path with the same rules as custom theme files. */
export function resolveTemplatePath(
  rawPath: string,
  configPath: string,
): string {
  return resolveConfigRelativePath(rawPath, configPath);
}
