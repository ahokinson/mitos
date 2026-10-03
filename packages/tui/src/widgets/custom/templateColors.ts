import type { ColorInput } from "@opentui/core";
import { approximateRgb, ColorRole, toColor } from "@theme/colors.ts";
import { Chrome, type Theme } from "@theme/themes.ts";
import Handlebars from "handlebars";

export type StyledRun = { text: string; fg?: ColorInput };

const MARK_OPEN = "\u0001";
const MARK_SEP = "\u0002";
const MARK_CLOSE = "\u0003";

/** A marker's payload is a color token (`#rrggbb`, `ansi:4`, `default`), or
 * several joined by commas for a gradient; none contain a marker character. */
const PAYLOAD = "[^\\u0001-\\u0003]+";
const OPEN_RE = new RegExp(`${MARK_OPEN}([cg])(${PAYLOAD})${MARK_SEP}`);
const TOKEN_PATTERN = `${MARK_OPEN}[cg]${PAYLOAD}${MARK_SEP}|${MARK_CLOSE}`;
const TOKEN_RE = new RegExp(TOKEN_PATTERN, "g");

/** What a template's color argument may name directly instead of a theme key. */
const LITERAL_PREFIXES = ["#", "ansi:"] as const;
const DEFAULT_TOKEN = "default";

function stripMarkers(value: string): string {
  return value.replace(TOKEN_RE, "");
}

/** A theme key resolves to that theme's token; a literal color is used as
 * written. An unknown or unset key falls back to the text color, then to the
 * terminal's default foreground. */
function resolveToken(token: string, theme: Theme): string {
  if (
    token === DEFAULT_TOKEN ||
    LITERAL_PREFIXES.some((prefix) => token.startsWith(prefix))
  )
    return token;
  return (
    (theme as Record<string, string | undefined>)[token] ??
    theme.text ??
    DEFAULT_TOKEN
  );
}

function hexFromChannels(r: number, g: number, b: number): string {
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n)));
  return `#${[r, g, b].map((n) => clamp(n).toString(16).padStart(2, "0")).join("")}`;
}

function gradientRuns(text: string, stops: readonly string[]): StyledRun[] {
  if (text.length === 0) return [];
  if (stops.length === 1)
    return [{ text, fg: toColor(stops[0], ColorRole.Foreground) }];
  const channels = stops.map((token) => approximateRgb(token));
  return [...text].map((char, index) => {
    const t = text.length === 1 ? 0 : index / (text.length - 1);
    const segment = t * (channels.length - 1);
    const lower = Math.min(Math.floor(segment), channels.length - 2);
    const localT = segment - lower;
    const [fromR = 0, fromG = 0, fromB = 0] = channels[lower] ?? [];
    const [toR = 0, toG = 0, toB = 0] = channels[lower + 1] ?? [];
    const fg = hexFromChannels(
      fromR + (toR - fromR) * localT,
      fromG + (toG - fromG) * localT,
      fromB + (toB - fromB) * localT,
    );
    return { text: char, fg };
  });
}

const NO_THEME: Theme = { chrome: Chrome.None };

function rootTheme(options: Handlebars.HelperOptions): Theme {
  return (
    (options.data?.root as { theme?: Theme } | undefined)?.theme ?? NO_THEME
  );
}

/** Block helpers `{{#color "accent"}}` and `{{#gradient "accent" "err"}}`.
 * They emit marker characters (U+0001..3) that `parseStyledLine` turns into
 * colored runs. Nesting is unsupported: the outer style wins. */
Handlebars.registerHelper(
  "color",
  function (this: unknown, token: unknown, options: Handlebars.HelperOptions) {
    const color = resolveToken(String(token), rootTheme(options));
    return new Handlebars.SafeString(
      `${MARK_OPEN}c${color}${MARK_SEP}${options.fn(this)}${MARK_CLOSE}`,
    );
  },
);

Handlebars.registerHelper(
  "gradient",
  function (this: unknown, ...args: unknown[]) {
    const options = args[args.length - 1] as Handlebars.HelperOptions;
    const theme = rootTheme(options);
    const colors = (args.slice(0, -1) as unknown[]).map((token) =>
      resolveToken(String(token), theme),
    );
    return new Handlebars.SafeString(
      `${MARK_OPEN}g${colors.join(",")}${MARK_SEP}${options.fn(this)}${MARK_CLOSE}`,
    );
  },
);

function parsePercent(value: string): number | null {
  const match = /(-?\d+(?:\.\d+)?)/.exec(value);
  if (!match) return null;
  const n = Number(match[1]);
  return Number.isFinite(n) ? Math.max(0, Math.min(100, n)) : null;
}

function severityColor(percent: number, theme: Theme): string {
  const fallback = theme.text ?? DEFAULT_TOKEN;
  if (percent >= 80) return theme.err ?? fallback;
  if (percent >= 50) return theme.warning ?? fallback;
  return theme.success ?? fallback;
}

/** `{{bar value 24}}`: a block bar sized to the first number in `value`,
 * clamped to 0-100 and colored by severity. No number renders an empty
 * `textDim` bar. */
Handlebars.registerHelper(
  "bar",
  function (
    this: unknown,
    value: unknown,
    width: unknown,
    options: Handlebars.HelperOptions,
  ) {
    const theme = rootTheme(options);
    const barWidth = Math.max(1, Math.round(Number(width)) || 20);
    const percent = parsePercent(String(value ?? ""));
    const filled =
      percent === null ? 0 : Math.round((percent / 100) * barWidth);
    const emptyColor = theme.textDim ?? theme.text ?? DEFAULT_TOKEN;
    const filledColor =
      percent === null ? emptyColor : severityColor(percent, theme);
    const filledChars = "█".repeat(filled);
    const emptyChars = "░".repeat(barWidth - filled);
    let out = "";
    if (filledChars.length > 0)
      out += `${MARK_OPEN}c${filledColor}${MARK_SEP}${filledChars}${MARK_CLOSE}`;
    if (emptyChars.length > 0)
      out += `${MARK_OPEN}c${emptyColor}${MARK_SEP}${emptyChars}${MARK_CLOSE}`;
    return new Handlebars.SafeString(out);
  },
);

/** Turns one rendered line into plain and colored runs; a line without
 * markers is a single run with no `fg`. */
export function parseStyledLine(line: string): StyledRun[] {
  const runs: StyledRun[] = [];
  let i = 0;
  while (i < line.length) {
    const rest = line.slice(i);
    const openMatch = OPEN_RE.exec(rest);
    if (!openMatch) {
      const text = stripMarkers(rest);
      if (text.length > 0) runs.push({ text });
      break;
    }
    const openStart = i + openMatch.index;
    if (openStart > i) {
      const text = stripMarkers(line.slice(i, openStart));
      if (text.length > 0) runs.push({ text });
    }
    const openEnd = openStart + openMatch[0].length;
    const kind = openMatch[1];
    const payload = openMatch[2] ?? "";
    let depth = 1;
    const scanRe = new RegExp(TOKEN_PATTERN, "g");
    scanRe.lastIndex = openEnd;
    let closeIndex = -1;
    for (let marker = scanRe.exec(line); marker; marker = scanRe.exec(line)) {
      if (marker[0] === MARK_CLOSE) {
        depth--;
        if (depth === 0) {
          closeIndex = marker.index;
          break;
        }
      } else {
        depth++;
      }
    }
    const rawInner =
      closeIndex >= 0 ? line.slice(openEnd, closeIndex) : line.slice(openEnd);
    const text = stripMarkers(rawInner);
    if (kind === "c") {
      if (text.length > 0)
        runs.push({ text, fg: toColor(payload, ColorRole.Foreground) });
    } else {
      runs.push(...gradientRuns(text, payload.split(",")));
    }
    i = closeIndex >= 0 ? closeIndex + 1 : line.length;
  }
  return runs;
}
