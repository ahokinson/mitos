import { useAppState } from "@app/states.tsx";
import { type PaneStyle, TextAlign } from "@layout/configs.ts";
import {
  buildMetricContext,
  type MetricContext,
} from "@panels/custom/metrics.ts";
import { parseStyledLine } from "@panels/custom/templateColors.ts";
import { useTheme, useThemeTokens } from "@theme/providers.tsx";
import { Chrome } from "@theme/themes.ts";
import Handlebars from "handlebars";
import { createMemo, For } from "solid-js";

/** Wraps non-empty leaves in `SafeString` to skip HTML escaping; an empty
 * value stays `""` so `{{#if}}` sees it as falsy. Arrays pass through as
 * arrays, with their own leaves wrapped, so `{{#each}}` can walk them.
 * Numbers stay numbers: they never need escaping, and `0` must stay
 * falsy for `{{#if}}`. */
function wrapSafe(value: unknown): unknown {
  if (typeof value === "number") return value;
  if (Array.isArray(value)) return value.map(wrapSafe);
  if (typeof value === "object" && value !== null) {
    const result: Record<string, unknown> = {};
    for (const [key, inner] of Object.entries(
      value as Record<string, unknown>,
    )) {
      result[key] = wrapSafe(inner);
    }
    return result;
  }
  const str = String(value ?? "");
  return str.length > 0 ? new Handlebars.SafeString(str) : "";
}

const ALIGN_ITEMS = {
  [TextAlign.Left]: "flex-start",
  [TextAlign.Center]: "center",
  [TextAlign.Right]: "flex-end",
} as const;

/** A read-only Handlebars view rendered against the live metrics registry,
 * one `<text>` per line with colored `<span>` runs. */
export function TemplatePanel(props: {
  paneStyle?: PaneStyle;
  template: string;
  align?: TextAlign;
}) {
  const { selected, currentHarness, usage, mining } = useAppState();
  const theme = useTheme();
  const tokens = useThemeTokens();
  const framed = () => theme.chrome === Chrome.Framed;

  const compiled = createMemo(() => Handlebars.compile(props.template));

  const styledLines = createMemo(() => {
    const render = compiled();
    const ctx: MetricContext = {
      selected: selected(),
      currentHarness: currentHarness(),
      usage: usage(),
      mining: mining(),
      workspaceRoot: process.env.MITOS_WORKSPACE_ROOT ?? null,
    };
    const wrapped = wrapSafe(buildMetricContext(ctx)) as Record<
      string,
      unknown
    >;
    const renderContext = { ...wrapped, theme: tokens };
    return render(renderContext).split("\n").map(parseStyledLine);
  });

  return (
    <box
      {...props.paneStyle}
      flexShrink={0}
      flexDirection="column"
      paddingX={1}
      border={framed()}
      borderColor={theme.textDim}
      alignItems={ALIGN_ITEMS[props.align ?? TextAlign.Left]}
    >
      <box flexDirection="column" flexShrink={0}>
        <For each={styledLines()}>
          {(runs) => (
            <text fg={theme.text}>
              <For each={runs}>
                {(run) => <span style={{ fg: run.fg }}>{run.text}</span>}
              </For>
            </text>
          )}
        </For>
      </box>
    </box>
  );
}
