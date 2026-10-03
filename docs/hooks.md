# Hooks

Harnesses report some things only through their own hooks, such as Claude's
account rate limits and live context window. `mitos hook <harness>` reads a
hook payload on stdin and records it. It never fails and does not slow the
harness. `/hooks` in the TUI shows each harness's hook status, and
`/hooks init [harness…]` installs them (`mitos hooks status` and
`mitos hooks init` do the same from a shell).

| Harness | Installed as | What the hook reports |
| --- | --- | --- |
| Claude Code | `statusLine` in `settings.json` | Session cost, context window, 5-hour and weekly limits, model |
| Codex | `SessionStart`, `Stop` and `SessionEnd` in `hooks.json` | Session id, cwd, model |
| Hermes | a `hooks:` block in `config.yaml` | Session id, cwd, model |
| OpenCode | a plugin file, `mitos.js`, in its plugin directory | Session id, cwd, model, running cost |

- **Claude's status line.** An existing `statusLine` command is kept: init
  rewrites it to `mitos hook claude --passthrough | <your command>`, so Mitos
  sees the JSON and your command still gets it unchanged. With no status line,
  the hook runs silently.
- **Backups.** Init writes `<file>.mitos-backup-<timestamp>` next to a file
  before changing it, never overwrites a file it cannot parse, and does nothing
  on a second run.
- **Read-only targets.** If Mitos cannot write a file, for example one managed
  by Nix, `/hooks` says so and shows the path, and init prints the exact entry
  to add instead. Hermes configs that already have a `hooks:` block are never
  edited; init prints the entries to add.
- **Approval.** Codex skips a new hook until you approve it with `/hooks`
  inside Codex, and Hermes asks on first use (or set `hooks_auto_accept: true`).
  `/hooks` lists such hooks as not yet approved. For Codex this is read from
  `config.toml`; the recorded hash itself is not verified.
- **Which thread.** Harnesses that Mitos launches interactively get
  `MITOS_THREAD_ID` in their environment, so a hook can tell which thread it belongs to.
  Hooks fired elsewhere still record account-wide data such as rate limits.
- **Debugging.** Set `MITOS_HOOK_DEBUG=1` to see why a hook recorded nothing.
- **Completion.** In the TUI, `/hooks` completes `status`, `init` and harness
  names. For shells, `source <(COMPLETE=zsh mitos)` (or `bash`, `fish`) completes
  subcommands and harness names for `hook`, `hooks init --harness` and
  `thread new --harness`.
