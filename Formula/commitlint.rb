class Commitlint < Formula
  desc "Native Conventional Commits message checker"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-only-aarch64-apple-darwin.tar.gz"
      sha256 "f4a17ae472ed6b09c42dd9b7f709102d213b34cb1366dbf9c11a6857e218d5e1"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-only-x86_64-apple-darwin.tar.gz"
      sha256 "40488b357a7bb4f693039c50d3a0b197973ac0bcefe6879e9542a9f1d415cdf9"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-only-aarch64-unknown-linux-musl.tar.gz"
      sha256 "a42bd4830ceafc1704681e74b5bb10e784d2af6450e767f3e5245a2812dfddbe"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-only-x86_64-unknown-linux-musl.tar.gz"
      sha256 "b1b5321009a45b49c615d3eeed6cd1dab890ed0450dd8949f47198b727aff0dc"
    end
  end

  def install
    bin.install "commitlint"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/commitlint --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint --help")
    assert_equal "", pipe_output("#{bin}/commitlint", "fix: validate standalone messages\n")
  end
end
