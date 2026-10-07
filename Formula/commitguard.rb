class Commitguard < Formula
  desc "Native Git and gh identity and outgoing commit guard"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitguard-aarch64-apple-darwin.tar.gz"
      sha256 "d7f5a3299e865e491f48bd327985295c7907d7a89f899e1ec34c4f6ce24e7541"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitguard-x86_64-apple-darwin.tar.gz"
      sha256 "3fa0affdae999ba190aa92654e32f6dc4c9593dbd007bffa5e69df34ad2f42c0"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitguard-aarch64-unknown-linux-musl.tar.gz"
      sha256 "a1edaaa46c25dfcd47fb0b6c38d6f58047276fc112e079587236a5ca706cca93"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitguard-x86_64-unknown-linux-musl.tar.gz"
      sha256 "73cada7f1e7912e1140c1f6aef620e7d592ff24fad8379de8875bdc77ad9e52b"
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
