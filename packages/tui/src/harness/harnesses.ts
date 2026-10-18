export enum KnownHarness {
  OpenCode = "opencode",
  Claude = "claude",
  Codex = "codex",
  Hermes = "hermes",
}

/** Full display list; `detectInstalledHarnesses` filters it by what is on
 * PATH. */
export const KNOWN_HARNESSES: readonly KnownHarness[] =
  Object.values(KnownHarness);

const HARNESS_BINARY_NAMES: Record<KnownHarness, string> = {
  [KnownHarness.Claude]: "claude",
  [KnownHarness.Codex]: "codex",
  [KnownHarness.OpenCode]: "opencode",
  [KnownHarness.Hermes]: "hermes",
};

/** Only installed harnesses are selectable. This probes the filesystem on
 * every call, so callers own the cadence: lifecycle points, not keystrokes. */
export function detectInstalledHarnesses(
  findBinary: (binary: string) => string | null = Bun.which,
): KnownHarness[] {
  return KNOWN_HARNESSES.filter(
    (harness) => findBinary(HARNESS_BINARY_NAMES[harness]) !== null,
  );
}
