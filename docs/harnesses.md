# Harnesses

Mitos drives the real agent CLI, so each harness keeps its own login and plan; Mitos never handles credentials. A harness shows up once its binary is on `PATH`.

| Harness | Binary | Headless turns | Plan mode | Session history read from |
| --- | --- | --- | --- | --- |
| Claude Code | `claude` | `claude -p` stream-json | yes | `~/.claude/projects` |
| Codex | `codex` | `codex app-server` | yes | `~/.codex/sessions` |
| OpenCode | `opencode` | private `opencode serve` over HTTP/SSE | yes (`plan` agent) | `$XDG_DATA_HOME/opencode/opencode-stable.db` |
| Hermes | `hermes` | `hermes acp` | no (build only) | `$HERMES_HOME/state.db` (`~/.hermes`) |

Each headless turn starts the harness, runs one turn, and stops it. OpenCode's server listens on a random loopback port behind a per-turn password. A thread that is in plan mode cannot be reassigned to Hermes.

`mitos enter` opens a harness interactively. OpenCode starts with the handoff as its prompt. Hermes cannot take an initial prompt and keep its REPL, so Mitos prints the handoff instead and a linked session resumes with `--resume`.
