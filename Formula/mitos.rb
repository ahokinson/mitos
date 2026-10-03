class Mitos < Formula
  desc "Session-aware handoff layer for native coding-agent harnesses"
  homepage "https://github.com/ahokinson/mitos"
  license "MIT"
  head "https://github.com/ahokinson/mitos.git", branch: "develop"

  depends_on "bun"
  depends_on "rust" => :build

  def install
    system "bun", "install", "--frozen-lockfile"
    system "bun", "run", "build"
    bin.install "dist/bin/mitos"
    lib.install "dist/lib/mitos"
  end

  def caveats
    <<~EOS
      mitos runs its TUI and harness adapters through bun at runtime
      (already installed as a dependency of this formula).
    EOS
  end

  test do
    assert_match "Keep the thread of work", shell_output("#{bin}/mitos --help")
  end
end
