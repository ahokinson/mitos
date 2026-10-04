# Installing

## Nix (flakes enabled)

```bash
nix build github:ahokinson/mitos
./result/bin/mitos
# or: nix run github:ahokinson/mitos
```

`nix develop` gives a shell with the Rust toolchain and `bun` for working
on Mitos itself. See `flake.nix` / `nix/package.nix`. The package vendors
`node_modules` through a fixed-output derivation, because Nix's sandboxed
builds have no network access. It is rebuilt only when `package.json` or
`bun.lock` changes.

## Homebrew

The formula lives at `Formula/mitos.rb`. It isn't tapped from this repo; check
for where it's published, or build it locally with
`brew install --build-from-source Formula/mitos.rb`:

```bash
brew install --HEAD ahokinson/mitos/mitos
```

## Layout

Either way, `bin/mitos` and `lib/mitos/` end up as siblings under the same
prefix, which is how `mitos` finds its own TUI at runtime
(`crates/mitos/src/cli/tuis.rs`). A packaged install needs no
`MITOS_LIBRARY_DIR` setting.

A packaged installation needs `bun` on `PATH` (or `MITOS_BUN`).
