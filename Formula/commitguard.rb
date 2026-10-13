class Commitguard < Formula
  desc "Native Git and gh identity and outgoing commit guard"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitguard-aarch64-apple-darwin.tar.gz"
      sha256 "8a061f71196caf2a39960e3616bb2f9991c12801799e3cdd76771b9ce12c87f8"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitguard-x86_64-apple-darwin.tar.gz"
      sha256 "5b299eda45603d85d121809d832580c71c0324051524b695e917dc1d08131c2b"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitguard-aarch64-unknown-linux-musl.tar.gz"
      sha256 "e2e47ed09c6bceaf432d1e0a687ef0a1b630684b59dde9dc210995249c591de1"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitguard-x86_64-unknown-linux-musl.tar.gz"
      sha256 "3d8dc40adaf721cfcee14fb0c838354fa82cce3452449e37085730de2312ca3b"
    end
  end

  depends_on "gh"
  depends_on "git"
  conflicts_with "commitlint-rust", because: "both provide the legacy guard command"

  def install
    bin.install "commitguard"
    bin.install_symlink "commitguard" => "gh-commit-guard"
  end

  def caveats
    <<~EOS
      Activate Git guards and Codex/Claude skills explicitly with:
        commitguard install
      Finish active history repairs before replacing their installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/commitguard version")
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
  end
end
