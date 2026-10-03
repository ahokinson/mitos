import { table, warn } from "@config/tomls.ts";
import { KNOWN_HARNESSES, type KnownHarness } from "@harness/harnesses.ts";
import {
  isKeyAction,
  isKeySpecString,
  type KeybindingsConfig,
} from "@input/keybindings.ts";
import type { ThemeConfig, ThemePathsConfig } from "@theme/themes.ts";
import { isPartialTheme } from "@theme/validators.ts";

export type SessionConfig = { defaultHarness?: KnownHarness };
export type HandoffConfig = { maxInlineBytes?: number };

export function validateThemeConfig(raw: unknown): ThemeConfig {
  const record = raw === undefined ? null : table(raw);
  if (raw !== undefined && !record) {
    warn("[theme] must be a table — using default theme");
    return {};
  }
  if (!record) return {};
  const name = typeof record.name === "string" ? record.name : undefined;
  const overrides = isPartialTheme(record.overrides)
    ? record.overrides
    : undefined;
  if (record.name !== undefined && !name)
    warn("[theme].name must be a string — ignoring");
  if (record.overrides !== undefined && !overrides)
    warn("[theme.overrides] has invalid tokens — ignoring");
  return { name, overrides };
}

export function validateThemePaths(raw: unknown): ThemePathsConfig {
  const record = raw === undefined ? null : table(raw);
  if (raw !== undefined && !record) {
    warn("[themes] must be a table — ignoring");
    return {};
  }
  return Object.fromEntries(
    Object.entries(record ?? {}).filter(([, path]) => typeof path === "string"),
  ) as ThemePathsConfig;
}

export function validateSession(raw: unknown): SessionConfig {
  const record = raw === undefined ? null : table(raw);
  if (raw !== undefined && !record) {
    warn("[session] must be a table — ignoring");
    return {};
  }
  const value = record?.default_harness;
  if (value === undefined) return {};
  if (
    typeof value === "string" &&
    (KNOWN_HARNESSES as readonly string[]).includes(value)
  )
    return { defaultHarness: value as KnownHarness };
  warn("[session].default_harness must name a known harness — ignoring");
  return {};
}

export function validateHandoff(raw: unknown): HandoffConfig {
  const record = raw === undefined ? null : table(raw);
  if (raw !== undefined && !record) {
    warn("[handoff] must be a table — using default limit");
    return {};
  }
  const value = record?.max_inline_bytes;
  if (value === undefined) return {};
  if (typeof value === "number" && Number.isInteger(value) && value > 0)
    return { maxInlineBytes: value };
  warn("[handoff].max_inline_bytes must be a positive integer — ignoring");
  return {};
}

export function validateKeybindings(raw: unknown): KeybindingsConfig {
  const record = raw === undefined ? null : table(raw);
  if (raw !== undefined && !record) {
    warn("[keybindings] must be a table — ignoring");
    return {};
  }
  const result: KeybindingsConfig = {};
  for (const [action, specs] of Object.entries(record ?? {})) {
    if (
      isKeyAction(action) &&
      Array.isArray(specs) &&
      specs.length > 0 &&
      specs.every(isKeySpecString)
    )
      result[action] = specs;
    else warn(`[keybindings].${action} is invalid — ignoring`);
  }
  return result;
}
