# Development

```bash
bun run check   # formatting, TypeScript checks, Clippy, and Rust tests
bun run coverage # TypeScript and Rust coverage; both enforce 95% thresholds
bun run build
```

`bun run coverage` requires `cargo-llvm-cov`. It is included by the Nix
development shell; other environments can install it with
`cargo install cargo-llvm-cov --locked`.

Tree-sitter parser binaries are sourced from the locked `tree-sitter-wasm`
development dependency when building. They are copied into release bundles but
are not stored in Git.

## CI reporting

GitHub Actions runs checks and coverage for pull requests targeting `develop`
and pushes to that branch. Coverage reports are published to Codecov only for
same-repository runs, so forks never receive the repository upload token.

Maintainers must connect the public repository in Codecov and add its upload
token as the `CODECOV_TOKEN` GitHub Actions secret. The separate OpenSSF
Scorecard workflow publishes a weekly security report and uploads its SARIF
result to GitHub Code Scanning. CodeQL scans the Rust and TypeScript sources on
pull requests, pushes, and weekly. Dependabot opens weekly update pull requests
for Bun, Cargo, and GitHub Actions dependencies.

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
