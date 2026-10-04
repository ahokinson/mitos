import { isColorToken } from "@theme/colors.ts";
import { Chrome, type Theme } from "@theme/themes.ts";

const COLOR_KEYS: readonly (keyof Theme)[] = [
  "backgroundChrome",
  "backgroundSelection",
  "text",
  "textMuted",
  "textDim",
  "accent",
  "success",
  "warning",
  "err",
];

const OPTIONAL_COLOR_KEYS: readonly (keyof Theme)[] = ["diffText"];

/** True when `value` has every required `Theme` key, each color a valid token. */
export function isTheme(value: unknown): value is Theme {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    COLOR_KEYS.every((key) => isColorToken(record[key])) &&
    OPTIONAL_COLOR_KEYS.every(
      (key) => record[key] === undefined || isColorToken(record[key]),
    ) &&
    Object.values(Chrome).includes(record.chrome as Chrome)
  );
}

/** True when every key is a `Theme` key holding a valid value. */
export function isPartialTheme(value: unknown): value is Partial<Theme> {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return Object.entries(record).every(([key, entry]) =>
    key === "chrome"
      ? typeof entry === "string"
      : ([...COLOR_KEYS, ...OPTIONAL_COLOR_KEYS] as readonly string[]).includes(
          key,
        ) && isColorToken(entry),
  );
}
