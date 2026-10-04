# Development

```bash
bun run check   # formatting, TypeScript checks, Clippy, and Rust tests
bun run build
```

For source-based development:

```bash
MITOS_SOURCE=/path/to/mitos
cd /path/to/workspace
MITOS_TUI=$MITOS_SOURCE/packages/tui/src/app/entries.tsx \
$MITOS_SOURCE/target/debug/mitos
```

The harness adapters (claude, codex, hermes, opencode) are part of the Rust
binary.

Release builds put the Bun TUI beside the Rust executable.
