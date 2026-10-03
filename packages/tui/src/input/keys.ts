import type { KeyEvent } from "@opentui/core";

export type KeySpec = {
  name?: string;
  sequence?: string;
  ctrl?: boolean;
  meta?: boolean;
  shift?: boolean;
};

export function matchKey(event: KeyEvent, spec: KeySpec): boolean {
  return (Object.keys(spec) as Array<keyof KeySpec>).every(
    (key) => event[key] === spec[key],
  );
}
