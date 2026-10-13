# Template panels

A panel with a `template` path is a read-only, hand-drawn view compiled with
[Handlebars](https://handlebarsjs.com/) and rendered against the metric
registry in `packages/tui/src/panels/custom/metrics.ts`. Template paths
expand `~` and otherwise resolve relative to `mitos.toml`. You draw the exact
layout you want as plain text and drop `{{metric.name}}` wherever a value
should go. No panel code is needed.

A layout holds **at most one** template panel: its file is the whole custom
view, read once at startup. Every panel you want — usage gauges, spend
totals, a harness comparison — is drawn in that one file, so the view lives
in one place instead of scattered across several templates. A config that
declares two template panels falls back to the default layout.

`packages/tui/examples/panels/` is a menu of complete views, each sized and
styled for a way of working. Copy the closest one and edit it:

| File | For who | What it shows |
| --- | --- | --- |
| `pilot.hbs` | the pilot — mid-flight | rate-window and context bars, reset times, current thread tokens and cost |
| `accountant.hbs` | the accountant | totals (messages by role, patches, files, forks), spend, cost by day, top projects |
| `optimizer.hbs` | the optimizer — deciding when to switch | live rate windows with time left and reset times, context and thread spend, a switch verdict, then the mined per-harness record |
| `profiler.hbs` | the profiler | tool call counts, seconds burned per tool, the aftermath of all that patching |
| `standup.hbs` | the standup narrator | recent sessions with harness, model and cost, pace of work, message volume |
| `minimalist.hbs` | the minimalist | thread facts, one usage strip, one lifetime line — quiet, not empty |

A template is wired in like this:

```toml
[panels.conversation]
kind = "conversation"
focus_key = "ctrl+1"

[panels.view]
template = "panels/optimizer.hbs"

[layout]
direction = "row"
children = [
  { panel = "conversation" },
  { panel = "view", size = 28 },
]
```

A template panel may add `align = "left"` (the default), `"center"` or
`"right"` to place its text block within the pane. The block moves as a
unit, so hand-drawn art keeps its shape. Template views cannot take focus.

## Metrics

Each line of the template file becomes one row. `{{harness}}`, `{{thread.id}}`,
`{{thread.status}}`, `{{thread.created}}`, `{{thread.updated}}`,
`{{workspace}}`, and the `{{usage.*}}` figures are all available. Tokens
(`usage.tokens_in`, `usage.tokens_out`, `usage.tokens_cached`) and
`usage.cost` are cumulative for the whole thread across every harness it has
used; a harness that reports no cost adds nothing to it. Context and rate
limits are the latest reading. `{{usage.model}}` names the model the live
harness reports. Plan limits come as `usage.plan_5h` and
`usage.plan_week` (rounded percent), plus `usage.plan_5h_resets` /
`usage.plan_week_resets` (local reset time) and `usage.plan_5h_remaining` /
`usage.plan_week_remaining` (time left). They appear only after the harness
has reported them, and a window whose reset time has passed reads as empty.

When the harness reports no windows, estimated Go plan windows ride the
mining report instead: `estimate.5h`, `estimate.week` and `estimate.month`
(percent of each window) and `estimate.5h_frees` (time until the oldest
spend ages out of the 5-hour window). They are computed from mined Go spend
against the model allowances the Go plan documents, cover only the machine
they were mined from, and render labeled `est` in the example views. The
`[go] tier` setting in `mitos.toml` — `go` (the default) or `plus` — picks
the tier's allowances.

A metric with no current value renders as an empty string, so the line shows
only the static text around it.

## Helpers

Two block helpers reach into the active theme for color:

```
{{#color "accent"}}{{harness}}{{/color}}
{{#gradient "accent" "err"}}{{thread.id}}{{/gradient}}
```

`color` paints its content in one theme color; `gradient` interpolates
across two or more, one step per character. The first argument to either is
a theme key (`text`, `textMuted`, `textDim`, `accent`, `success`,
`warning`, `err`, and so on) or a literal color: `#rrggbb`, `ansi:N`, or
`default`. They don't nest: wrapping one inside the other flattens the inner
block to plain text.

`{{bar value 20}}` draws a filled/empty block bar from whatever number it
finds in `value` (a bare number or one followed by `%`), clamped to 0-100,
colored by severity (success under 50, warning under 80, err at or above).
No parseable number renders as an empty, dim bar, so you can point it at a
metric that hasn't reported yet.

`{{#gte value threshold}}` renders its block when the first number in
`value` (bare or `N%`) is at least `threshold`; `{{else}}` renders
otherwise. No parseable number counts as below, so a warning can wait for
data that has not arrived:

```
{{#gte usage.plan_5h 70}}window at {{usage.plan_5h}} — time to switch{{else}}healthy{{/gte}}
```

Three formatting helpers dress raw series values, which arrive as plain
numbers: `{{usd value}}` (money, keeping four decimals below a cent),
`{{compact value}}` (`184M`-style counts) and `{{duration value}}`
(seconds as `2h 50m`). Each renders an empty string for no parseable
number.

`{{clip value 32}}` clips the value's first line to 32 columns, ending in
`…` when it cut — the tool that keeps a long session title or project
path from wrapping a row. A value within the width, or none at all,
passes through untouched.

Ordinary Handlebars conditionals (`{{#if}}`, `{{#unless}}`, `{{#each}}`)
work too. A metric with no value is an empty string, which is falsy, so a
template can branch on whether data has arrived.

## Chart panels

A panel with a `kind` of `bars`, `spark`, `heatmap` or `sessions` draws one
mined series as a read-only chart. `metric` names the series; `top` trims
the rows drawn. A `usage` chart takes neither: it draws the active
harness's live rate windows and context as severity bars, with the time
each window ends.

```toml
[panels.spend]
kind = "bars"
metric = "cost_by_project"
top = 8

[panels.recent]
kind = "sessions"
top = 5
```

`bars` and `spark` take any grouped series: `cost_by_day`,
`cost_by_project`, `cost_by_model`, `cost_by_agent`, `cost_by_harness`,
`sessions_by_agent`, `sessions_by_model`, `tool_usage`, `tool_time`;
`spark` also takes the `sessions_by_day` day series and draws its newest
`top` points. `heatmap` takes `activity_by_day` only and draws one ramped
row per day, one cell per hour (UTC). `sessions` takes no metric and lists
recent sessions with their harness, cost and tokens. Charts size their
columns to the pane they land in — labels clip, bars shrink and the
session table drops its tokens column before its harness column when the
pane is tight. Chart views are read-only, cannot take focus, and never
declare `template`, `align` or `focus_key`. A chart with no history yet
draws one muted line instead of an empty pane; a `usage` chart without a
snapshot yet says it is waiting for the first turn.

A complete starter layout — the conversation pane flanked by the live
usage chart over the mined history charts — copies as-is into `mitos.toml`:

```toml
[panels.conversation]
kind = "conversation"
focus_key = "ctrl+1"

[panels.usage]
kind = "usage"

[panels.trend]
kind = "spark"
metric = "cost_by_day"

[panels.spend]
kind = "bars"
metric = "cost_by_harness"
top = 4

[panels.recent]
kind = "sessions"
top = 12

[layout]
direction = "row"
children = [
  { panel = "conversation", size = "80%" },
  { direction = "column", size = "20%", children = [
    { panel = "usage", size = 5 },
    { panel = "trend", size = 5 },
    { panel = "spend", size = 7 },
    { panel = "recent" },
  ] },
]
```

## Mining metrics

The figures come from `mitos view mining`, which mines each harness's own
stored history and merges them: OpenCode's SQLite, Claude's session JSONL
(`~/.claude/projects`, subagent sidechains excluded), and Codex rollout
transcripts (`~/.codex/sessions`). A harness without a minable history
(Hermes today) contributes nothing, so the charts update live as turns land.

Templates can read the merged history directly. Scalars: `{{mining.sessions}}`,
`{{mining.messages}}`, `{{mining.messages_user}}` / `{{mining.messages_assistant}}`,
`{{mining.tool_calls}}`, `{{mining.patches}}`, `{{mining.files_patched}}`,
`{{mining.forks}}`, `{{mining.cost}}` (total spent), `{{mining.tool_seconds}}`
(total tool time, formatted like `1d 3h`), and the token columns
`{{mining.tokens_in}}` / `{{mining.tokens_out}}` / `{{mining.tokens_reasoning}}`
/ `{{mining.tokens_cached}}` / `{{mining.tokens_cache_write}}`.

The series stay arrays, so `{{#each}}` walks them:

```
{{#each mining.harnesses}}
{{this.label}} — {{this.sessions}} sessions, {{usd this.cost_usd}}
{{/each}}
```

- `mining.harnesses` — per-harness aggregates (`label`, `sessions`,
  `messages`, `tool_calls`, `patches`, `cost_usd`, `tokens_input`,
  `tokens_output`, `tokens_cache_read`): the comparison an optimizer tunes a
  switch decision with.
- `mining.harness_costs`, `mining.projects`, `mining.models` and
  `mining.agents` — grouped cost series (`{label, value}`).
- `mining.agent_sessions`, `mining.model_sessions` and `mining.tools` —
  grouped counts.
- `mining.days`, `mining.session_days` — day series (cost and session count).
- `mining.tool_time` — seconds burned per tool.
- `mining.activity` — the heatmap's day rows.
- `mining.recent` — the session list; each entry also carries `harness`,
  `cost_usd`, `tokens_*`, `model` and `agent`.

Every series is capped at 100 rows per harness before the merge; a template
walking a long series should expect the provider ceiling, and a chart trims
to its own `top`. The example views draw complete panels from these —
`optimizer.hbs` is the fullest walkthrough.
