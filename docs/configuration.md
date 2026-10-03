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
# default_harness = "codex"

[handoff]
max_inline_bytes = 65536

[widgets.conversation]
kind = "conversation"
focus_key = "ctrl+1"

[layout]
direction = "row"
children = [
  { widget = "conversation" },
]
```

Widget IDs are names you choose. `conversation` is the only interactive
built-in widget kind. For hand-drawn panels, see [Template widgets](widgets.md).
