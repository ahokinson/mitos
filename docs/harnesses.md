# Harnesses

Mitos drives the real agent CLI, so each harness keeps its own login; Mitos never handles credentials. A harness shows up once its binary is on `PATH`. A plan is a separate subscription pool: OpenCode can consume several plans, and a ChatGPT plan can be consumed through both OpenCode and Codex. OpenCode is the preferred harness: when several are installed and `[session].default_harness` is not set, a new thread starts on OpenCode.

| Harness | Binary | Headless turns | Plan mode | Session history read from |
| --- | --- | --- | --- | --- |
| OpenCode | `opencode` | private `opencode serve` over HTTP/SSE | yes (`plan` agent) | `$XDG_DATA_HOME/opencode/opencode-stable.db` |
| Claude Code | `claude` | `claude -p` stream-json | yes | `~/.claude/projects` |
| Codex | `codex` | `codex app-server` | yes | `~/.codex/sessions` |
| Hermes | `hermes` | `hermes acp` | no (build only) | `$HERMES_HOME/state.db` (`~/.hermes`) |

Each headless turn starts the harness, runs one turn, and stops it. The OpenCode server is the exception: `mitos daemon` keeps one shared server alive across turns, and each turn connects to it over HTTP/SSE. The server listens on a random loopback port behind a password. A thread that is in plan mode cannot be reassigned to Hermes.

`mitos enter` opens a harness interactively. OpenCode starts with the handoff as its prompt. Hermes cannot take an initial prompt and keep its REPL, so Mitos prints the handoff instead and a linked session resumes with `--resume`.

## OpenCode first class

### Shared server

`mitos daemon start` starts one OpenCode server for the whole user; the first OpenCode turn starts it automatically when none is running. Every turn connects to it, so the server (and its plugins) starts once instead of once per turn. The server stops on its own after 15 minutes without a turn; `mitos daemon stop` stops it immediately, and `mitos daemon status` reports it.

The state lives beside the Mitos database: `$XDG_CONFIG_HOME/mitos/opencode-server.json` (mode `0600`) records the URL, the password, and both process ids. Set `MITOS_OPENCODE_SERVER=per-turn` to skip the shared server and start a private one per turn, as before; set `MITOS_OPENCODE_IDLE_SECS` to change the idle limit.

### Session control

| Command | OpenCode endpoint | Effect |
| --- | --- | --- |
| `/abort`, `mitos thread abort <id>` | `POST /session/:id/abort` | Stops the thread's running turn |
| `/fork`, `mitos thread fork <id>` | `POST /session/:id/fork` | New Mitos thread with the session's history |
| `/share`, `mitos thread share <id>` | `POST /session/:id/share` | Prints the session's public link |
| `/diff`, `mitos thread diff <id>` | `GET /session/:id/diff` | Lists the files the session changed |

`/share` publishes the conversation to opencode.ai; it runs only when you ask. These commands need the shared server and work on OpenCode threads only.

### Agents and models

A thread's mode stays the safety boundary. On top of it, a thread can name an agent and a model:

- `/agent <name>` sets the OpenCode agent for the thread's turns (any agent the server offers, including your own from `.opencode/agents/`); bare `/agent` lists them. No name returns the thread to its mode's default agent.
- `/model <provider/model>` sets the model; bare `/model` lists the server's defaults. No reference returns the thread to the harness default.
- `/oc <command> [args…]` runs an OpenCode slash command in the thread's session and prints the reply.

Other harnesses ignore the agent and model settings. Undo (`/undo`) is not wired: OpenCode's revert works against its own history, while a Mitos thread's view is an append-only event log, so reverting needs a transcript resync design and comes later.
