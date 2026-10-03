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
MITOS_ADAPTER_DIR=$MITOS_SOURCE/packages/adapters/src/entrypoints \
$MITOS_SOURCE/target/debug/mitos
```

Release builds put the Bun TUI and adapters beside the Rust executable.
