<pre align="center">
█▀▄▀█ █ ▀█▀ █▀█ █▀▀
█░▀░█ █  █  █░█ ▀▀█
▀   ▀ ▀  ▀  ▀▀▀ ▀▀▀
────────────◈──────
the golden thread
</pre>

[![CI](https://github.com/ahokinson/mitos/actions/workflows/ci.yml/badge.svg?branch=develop&event=push)](https://github.com/ahokinson/mitos/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ahokinson/mitos/branch/develop/graph/badge.svg)](https://codecov.io/gh/ahokinson/mitos)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/ahokinson/mitos/badge)](https://scorecard.dev/viewer/?uri=github.com/ahokinson/mitos)

Mitos is a small terminal interface for carrying one coding conversation across native agent harnesses. A session belongs to the current workspace, starts as an empty draft, and is created only when you send its first message.

The initial interface shows only the conversation. There is no dashboard and no required setup.

```bash
bun install
bun run build

cd /path/to/workspace
/path/to/mitos/dist/bin/mitos
```

Mitos drives the real agent CLI (Claude Code, Codex, OpenCode, Hermes), so each harness keeps its own login and plan.

## Docs

- [Installing](docs/installing.md): Nix, Homebrew, install layout
- [Sessions and handoffs](docs/sessions.md): keys, commands, switching harnesses
- [Harnesses](docs/harnesses.md): supported CLIs and how each is driven
- [Hooks](docs/hooks.md): account limits and context reporting
- [Configuration](docs/configuration.md): `mitos.toml`, themes, layout
- [Template widgets](docs/widgets.md): hand-drawn Handlebars panels
- [Development](docs/development.md): checks, builds, running from source
