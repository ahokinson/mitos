# Template widgets

A widget with a `template` path is a read-only,
hand-drawn view compiled with [Handlebars](https://handlebarsjs.com/) and
rendered against the metric registry in `packages/tui/src/widgets/custom/metrics.ts`
(harness, thread id/status/timestamps, workspace, usage/context/token/
rate-limit figures). Template paths expand `~` and otherwise resolve relative
to `mitos.toml`. You draw the exact layout you want as plain text and drop
`{{metric.name}}` wherever a value should go. No widget code is needed.
`packages/tui/examples/widgets/` has example templates you can copy:
`metadata.hbs` (thread info), `usage.hbs` (session/week limit bars with reset
times, context, cost), `usage-limits.hbs` (two-line limits strip) and
`usage-tokens.hbs` (tokens, cost, context). A template is wired in like this:

```toml
[widgets.usage]
template = "widgets/usage.hbs"

[layout]
direction = "row"
children = [
  { widget = "conversation" },
  { widget = "usage", size = 30 },
]
```

A template widget may add `align = "left"` (the default), `"center"` or
`"right"` to place its text block within the pane. The block moves as a unit,
so hand-drawn art such as `logo.hbs` keeps its shape.

## Metrics

Each line of the template file becomes one row. `{{harness}}`, `{{thread.id}}`,
`{{thread.status}}`, `{{thread.created}}`, `{{thread.updated}}`,
`{{workspace}}`, and the `{{usage.*}}` figures are all available. Tokens
(`usage.tokens_in`, `usage.tokens_out`, `usage.tokens_cached`) and
`usage.cost` are cumulative for the whole thread across every harness it has
used; a harness that reports no cost adds nothing to it. Context and rate
limits are the latest reading. Plan limits come as `usage.plan_5h` and
`usage.plan_week` (rounded percent), plus `usage.plan_5h_resets` /
`usage.plan_week_resets` (local reset time) and `usage.plan_5h_remaining` /
`usage.plan_week_remaining` (time left). They appear only after the harness
has reported them (Claude's statusLine hook, Codex transcripts at handoff),
and a window whose reset time has passed reads as empty. A metric with no current value (nothing
selected yet, no usage data yet, etc.) renders as an empty string, so the
line shows only the static text around it. The `.hbs` files in
`packages/tui/examples/widgets/` are complete layouts, including multi-line
sections and literal box-drawing characters. Template views cannot take focus.

## Helpers

Two block helpers reach into the active theme for color:

```
{{#color "accent"}}{{harness}}{{/color}}
{{#gradient "accent" "err"}}{{thread.id}}{{/gradient}}
```

`color` paints its content in one theme color; `gradient` interpolates
across two or more, one step per character. The first argument to
either is a theme key (`text`, `textMuted`, `textDim`, `accent`, `success`,
`warning`, `err`, and so on) or a literal color: `#rrggbb`, `ansi:N`, or `default`.
A gradient has to blend real channels, so palette stops are blended from
the standard xterm values for those slots; a solid `color` keeps the terminal's
own palette color. They don't nest: wrapping one inside the other flattens
the inner block to plain text and the outer color applies to the whole span.

A third helper, `{{bar usage.context_percent 20}}`, draws a filled/empty
block bar from whatever number it finds in its first argument (a bare
number or one followed by `%`), sized to its second argument, colored by
severity (`success` under 50, `warning` under 80, `err` at or above). No
parseable number renders as an empty, dim bar instead of an error, so you can
point it at a metric that hasn't reported yet. Ordinary Handlebars
conditionals (`{{#if usage.harness}}...{{else}}...{{/if}}`) work too. A
metric with no value is an empty string, which is falsy, so a template can
branch on whether data has arrived. `usage.hbs` uses all of this: per-rate-limit
and context bars that only appear when the harness reports them, thread token
totals, and a "waiting for data"/"not reported" fallback.
