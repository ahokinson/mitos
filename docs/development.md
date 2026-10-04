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

The harness adapters are part of the Rust binary. `MITOS_ADAPTER_DIR` (or
`--adapter-dir`) points at a directory of external adapters instead, one
`mitos-<harness>` executable per harness, which replaces the built-in ones.

Release builds put the Bun TUI beside the Rust executable.
