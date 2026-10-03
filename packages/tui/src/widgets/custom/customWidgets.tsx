import Handlebars from "handlebars"
import { For, createMemo } from "solid-js"

import { type PaneStyle, TextAlign } from "@layout/trees.ts"
import { buildMetricContext, type MetricContext } from "@widgets/custom/metrics.ts"
import { parseStyledLine } from "@widgets/custom/templateColors.ts"
import { useAppState } from "@app/states.tsx"
import { Chrome } from "@theme/themes.ts"
import { useTheme, useThemeTokens } from "@theme/providers.tsx"

/** Wraps non-empty leaves in `SafeString` to skip HTML escaping; an empty
 * value stays `""` so `{{#if}}` sees it as falsy. */
function wrapSafe(context: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(context)) {
    if (typeof value === "object" && value !== null) {
      result[key] = wrapSafe(value as Record<string, unknown>)
      continue
    }
    const str = String(value ?? "")
    result[key] = str.length > 0 ? new Handlebars.SafeString(str) : ""
  }
  return result
}

const ALIGN_ITEMS = {
  [TextAlign.Left]: "flex-start",
  [TextAlign.Center]: "center",
  [TextAlign.Right]: "flex-end",
} as const

/** A read-only Handlebars view rendered against the live metrics registry,
 * one `<text>` per line with colored `<span>` runs. */
export function TemplateWidget(props: {
  paneStyle?: PaneStyle
  template: string
  align?: TextAlign
}) {
  const { selected, currentHarness, usage } = useAppState()
  const theme = useTheme()
  const tokens = useThemeTokens()
  const framed = () => theme.chrome === Chrome.Framed

  const compiled = createMemo(() => Handlebars.compile(props.template))

  const styledLines = createMemo(() => {
    const render = compiled()
    const ctx: MetricContext = {
      selected: selected(),
      currentHarness: currentHarness(),
      usage: usage(),
      workspaceRoot: process.env.MITOS_WORKSPACE_ROOT ?? null,
    }
    const renderContext = { ...wrapSafe(buildMetricContext(ctx)), theme: tokens }
    return render(renderContext)
      .split("\n")
      .map(parseStyledLine)
  })

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
              <For each={runs}>{(run) => <span style={{ fg: run.fg }}>{run.text}</span>}</For>
            </text>
          )}
        </For>
      </box>
    </box>
  )
}
