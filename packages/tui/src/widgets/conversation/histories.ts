export type InputHistory = {
  prev: (current: string) => string | undefined;
  next: (current: string) => string | undefined;
  reset: () => void;
};

/** Recall starts from an empty composer and continues while the text is still
 * the entry last recalled; any edit ends it. Entries load fresh at each start. */
export function createInputHistory(
  load: () => readonly string[],
): InputHistory {
  let entries: readonly string[] = [];
  let index = -1;

  const shown = (): string | undefined =>
    index >= 0 ? entries[index] : undefined;
  const browsing = (current: string): boolean =>
    index >= 0 && current === shown();

  function reset(): void {
    entries = [];
    index = -1;
  }

  function prev(current: string): string | undefined {
    if (!browsing(current)) {
      if (current !== "") return undefined;
      entries = load();
      index = -1;
    }
    if (index + 1 >= entries.length) return undefined;
    index += 1;
    return shown();
  }

  function next(current: string): string | undefined {
    if (!browsing(current)) return undefined;
    index -= 1;
    return shown() ?? "";
  }

  return { prev, next, reset };
}
