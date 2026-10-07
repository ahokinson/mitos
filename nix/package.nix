{
  lib,
  stdenvNoCC,
  rustPlatform,
  runCommand,
  bun,
  cacert,
  git,
  makeWrapper,
}:

let
  version = "0.1.0";

  # Nix's own directory-copy semantics have no concept of `.gitignore`, and
  # this repo's root currently also holds plenty that has nothing to do with
  # the package (shell rc files, editor state, agent session data) — so
  # `src` is an explicit allowlist of exactly what each build needs, never
  # a raw `../.`. These two also stay disjoint from node_modules/dist/target
  # entirely, since none of those are ever read as a *source*.
  rustSrc = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
    ];
  };

  tuiSrc = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../package.json
      ../bun.lock
      ../scripts
      ../packages/tui
    ];
  };

  # `bun install` resolves platform-specific optional deps, so the vendored
  # tree hashes differently per system. Every supported system needs an entry.
  nodeModulesHashes = {
    aarch64-darwin = "sha256-Y24JsZ2cjIcGdZDHVFj5L6Mzr3eUWIfJiTHIotpBEJo=";
    aarch64-linux = lib.fakeHash;
    x86_64-darwin = lib.fakeHash;
    x86_64-linux = "sha256-mWZRSTG5TRvzpUwI1RGz5/l89Z7GI6BJqi5rSbuQ6xA=";
  };

  # Only the manifests `bun install` reads, not the full `tuiSrc` tree — so
  # editing application source doesn't change this derivation's input and
  # force a pointless re-fetch of the exact same dependency set.
  bunManifest = runCommand "mitos-bun-manifest" { } ''
    mkdir -p $out/packages/tui
    cp ${../package.json} $out/package.json
    cp ${../bun.lock} $out/bun.lock
    cp ${../packages/tui/package.json} $out/packages/tui/package.json
  '';

  # A sandboxed Nix build has no network access except inside a
  # fixed-output derivation, which is allowed to reach it because it
  # declares the hash it expects up front — the same trick fetchNpmDeps and
  # fetchCargoVendor use for npm/cargo. There's no nixpkgs-provided
  # equivalent for Bun yet, so this vendors `node_modules` by hand. First
  # build: bump `outputHash` to whatever mismatch error Nix reports.
  nodeModules = stdenvNoCC.mkDerivation {
    pname = "mitos-node-modules";
    inherit version;
    src = bunManifest;
    nativeBuildInputs = [
      bun
      cacert
    ];
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
      runHook preBuild
      export HOME="$TMPDIR"
      bun install --frozen-lockfile --ignore-scripts
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p "$out/packages/tui"
      cp -r node_modules "$out/"
      cp -r packages/tui/node_modules "$out/packages/tui/"
    '';
    outputHashMode = "recursive";
    outputHash =
      nodeModulesHashes.${stdenvNoCC.hostPlatform.system}
        or (throw "mitos: no node_modules hash for ${stdenvNoCC.hostPlatform.system}");
  };

  # Plain `cargoLock` (not `cargoHash`) trusts the checksums already pinned
  # in the project's own Cargo.lock, so there's no separate hash for us to
  # compute or keep in sync here — only the hand-rolled Bun FOD above needs
  # that dance, because cargo's lockfile mechanism already does it for Rust.
  mitosCore = rustPlatform.buildRustPackage {
    pname = "mitos";
    inherit version;
    src = rustSrc;
    cargoLock = {
      lockFile = ../Cargo.lock;
    };
    cargoBuildFlags = [
      "-p"
      "mitos"
    ];
    nativeCheckInputs = [ git ];
  };
in
stdenvNoCC.mkDerivation {
  pname = "mitos";
  inherit version;
  src = tuiSrc;
  nativeBuildInputs = [
    bun
    makeWrapper
  ];
  dontConfigure = true;

  buildPhase = ''
    runHook preBuild
    export HOME="$TMPDIR"
    ln -s ${nodeModules}/node_modules ./node_modules
    ln -s ${nodeModules}/packages/tui/node_modules packages/tui/node_modules
    export MITOS_VENDORED_NODE_MODULES=${nodeModules}/node_modules
    export MITOS_SKIP_CARGO=1
    bun run scripts/build.ts
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin" "$out/lib/mitos"
    cp ${mitosCore}/bin/mitos "$out/bin/mitos"
    cp -r dist/lib/mitos/. "$out/lib/mitos/"
    runHook postInstall
  '';

  # mitos resolves its TUI directory from its own executable's
  # path (bin/../lib/mitos — see crates/mitos/src/cli/tuis.rs), which
  # survives wrapProgram's rename-and-wrap trick since the wrapper keeps the
  # real binary in the same bin/ directory. MITOS_BUN pins the exact bun
  # this package was built against instead of trusting the caller's PATH.
  postFixup = ''
    wrapProgram "$out/bin/mitos" --set MITOS_BUN "${bun}/bin/bun"
  '';

  meta = {
    description = "A session-aware handoff layer for native coding-agent harnesses";
    homepage = "https://github.com/ahokinson/mitos";
    license = lib.licenses.mit;
    mainProgram = "mitos";
    platforms = lib.platforms.unix;
  };
}
