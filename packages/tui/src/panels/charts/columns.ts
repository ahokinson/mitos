import { type MetricValue, shortenHome } from "@panels/custom/metrics.ts";
import type { Theme } from "@theme/themes.ts";

export const DEFAULT_TOP = 10;
const BAR_MIN = 4;
const BAR_MAX = 16;
const LABEL_MIN = 8;
const LABEL_MAX = 24;
const VALUE_SPAN = 10;
export const DAY_COL = 6;
export const HARNESS_COL = 10;
const COST_SPAN = 8;
const TOKENS_SPAN = 6;
const TITLE_MIN = 8;
const TITLE_MAX = 24;
export const SPARK_STEPS = "▁▂▃▄▅▆▇█";

/** The pane's usable columns: measured width minus the frame's border and
 * padding. An unmeasured or degenerate pane leaves the default widths. */
export function innerWidth(
  paneWidth: number | undefined,
  framed: boolean,
): number | undefined {
  if (paneWidth === undefined || paneWidth <= 0) return undefined;
  return Math.max(8, paneWidth - (framed ? 4 : 2));
}

export function clipLabel(label: string, width: number): string {
  const shortened = shortenHome(label);
  return shortened.length > width
    ? `${shortened.slice(0, width - 1)}…`
    : shortened;
}

/** A bar row's label and bar widths: label and bar share the inner
 * columns in their default proportion, each holding to its floor. */
export function barColumns(inner: number | undefined): {
  label: number;
  bar: number;
} {
  if (inner === undefined) return { label: LABEL_MAX, bar: BAR_MAX };
  const available = Math.max(LABEL_MIN + BAR_MIN, inner - 1 - VALUE_SPAN);
  const share = available / (LABEL_MAX + BAR_MAX);
  const label = Math.min(
    LABEL_MAX,
    Math.max(LABEL_MIN, Math.round(LABEL_MAX * share)),
  );
  const bar = Math.min(BAR_MAX, Math.max(BAR_MIN, available - label));
  return { label, bar };
}

/** A session row's columns: the title keeps its share, and the tokens
 * column drops first when the pane is too tight to keep everything. */
export function sessionColumns(inner: number | undefined): {
  title: number;
  harness: boolean;
  tokens: boolean;
} {
  if (inner === undefined)
    return { title: TITLE_MAX, harness: true, tokens: true };
  const withTokens =
    inner - DAY_COL - 1 - HARNESS_COL - 1 - COST_SPAN - TOKENS_SPAN;
  if (withTokens >= TITLE_MIN)
    return {
      title: Math.min(TITLE_MAX, withTokens),
      harness: true,
      tokens: true,
    };
  const withoutTokens = inner - DAY_COL - 1 - HARNESS_COL - COST_SPAN;
  if (withoutTokens >= TITLE_MIN)
    return {
      title: Math.min(TITLE_MAX, withoutTokens),
      harness: true,
      tokens: false,
    };
  return {
    title: Math.max(
      TITLE_MIN,
      Math.min(TITLE_MAX, inner - DAY_COL - 1 - COST_SPAN),
    ),
    harness: false,
    tokens: false,
  };
}

/** Tone by relative magnitude, the same severity bands the `bar` helper uses. */
export function magnitudeTone(theme: Theme, ratio: number): string | undefined {
  if (ratio >= 0.8) return theme.err;
  if (ratio >= 0.5) return theme.warning;
  return undefined;
}

export function sparkStep(value: number, max: number): number {
  return Math.round((value / max) * (SPARK_STEPS.length - 1));
}

/** The percent behind a formatted `usage.*` metric, like `74%` → 74. */
export function percentOf(metric: MetricValue | null): number | null {
  if (!metric) return null;
  const n = Number(metric.value.replace("%", ""));
  return Number.isFinite(n) ? n : null;
}

/** Tail parts join with ` · ` while the pane still fits them. */
export function fitTail(
  parts: string[],
  inner: number | undefined,
  used: number,
): string {
  let tail = "";
  for (const part of parts) {
    const piece = tail.length === 0 ? part : ` · ${part}`;
    if (inner === undefined || used + tail.length + piece.length <= inner) {
      tail += piece;
    }
  }
  return tail;
}
