# Configuration

Mitos creates its personal configuration at `$XDG_CONFIG_HOME/mitos/mitos.toml` (or `~/.config/mitos/mitos.toml`). The built-in theme is named `default` and is terminal-native: text is the terminal's own foreground, no background is painted, and the accent and status colors come from the terminal's ANSI palette, so they follow whatever color scheme the terminal uses. Every theme leaves the base background to the terminal, so a translucent or blurred terminal emulator shows through whatever the theme.

A theme color is one of:

| Token | Meaning |
| --- | --- |
| `#rrggbb` (or a CSS name) | A fixed color |
| `ansi:4`, `ansi:blue`, `ansi:bright-black` | The terminal's own palette entry, 0-255 or one of the 16 names |
| `default` | The terminal's default foreground, or its default background for the two `background…` tokens |

The named themes (`mocha`, `latte`, `rose-pine-moon`) are fixed hex. A custom theme (which must define every token) or a `[theme.overrides]` entry can mix all three; an override leaves any token it omits as the theme has it.

```toml
[theme]
name = "default"

# [session]
# default_harness = "opencode"

[handoff]
max_inline_bytes = 65536

[plans]
opencode_go = "plus"
opencode_zen = true
chatgpt = "plus"

[panels.conversation]
kind = "conversation"
focus_key = "ctrl+1"

[layout]
direction = "row"
children = [
  { panel = "conversation" },
]
```

`[handoff].max_inline_bytes` caps the handoff Mitos gives a harness (default 256 KiB). When it runs over, tool output goes first, then tool calls and status lines; messages, decisions, questions and the latest summary are kept, and only if they alone still don't fit does the middle get cut. `mitos enter` passes the handoff on the harness command line, so it is held to at most 120 KiB regardless of this setting.

`[plans]` enables the subscription pools Mitos may attribute usage to. The
supported plans are `opencode_go = "go"` or `"plus"`, `opencode_zen = true`,
and `chatgpt = "plus"` or `"pro"`. Mitos matches them against fixed sources: OpenCode's
`opencode-go`, `opencode`, and `openai` providers; direct Codex also uses
`chatgpt-plus` or `chatgpt-pro`. A harness is only the program that runs a turn. It is not the
subscription that pays for it.

Panel IDs are names you choose. `conversation` is the only interactive
built-in panel kind. Read-only chart kinds draw mined session history, and
a hand-drawn panel comes from a template; both are covered in
[Template panels](panels.md). A layout declares at most one template
panel — its file is the whole custom view, read once.
