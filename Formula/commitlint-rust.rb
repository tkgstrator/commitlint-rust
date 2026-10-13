class CommitlintRust < Formula
  desc "Compatible combined lint and Git identity guard distribution"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-rust-aarch64-apple-darwin.tar.gz"
      sha256 "09f29f0b10ed8c5c09b5d382b0087504088f7a685f3e30652bd36f244e8095b5"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-rust-x86_64-apple-darwin.tar.gz"
      sha256 "53e3a383d3293db92eb222cdf0b8e1da70ab20a4661cbb8640310b387045df61"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-rust-aarch64-unknown-linux-musl.tar.gz"
      sha256 "7760c071eece1790fc0fec6fadb411f2bbb1d459e06922b91fc0ae511b42e4ec"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.3.0/commitlint-rust-x86_64-unknown-linux-musl.tar.gz"
      sha256 "f98bbd1fdf96ff19b3a07331b121cb8b21334d36fc6180ef8bbc6ebb6659bb53"
    end
  end

  depends_on "gh"
  depends_on "git"
  conflicts_with "commitguard", because: "both provide the legacy guard command"

  def install
    bin.install "gh-commit-guard"
    bin.install_symlink "gh-commit-guard" => "commitguard"
    bin.install "commitlint" => "commitlint-rust"
  end

  def caveats
    <<~EOS
      This combined compatibility package preserves commands from v0.1.0.
      New installations can choose the independent commitlint and commitguard formulae.
      Guard activation stays explicit: commitguard install
      Finish active history repairs before replacing their installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
    assert_match version.to_s, shell_output("#{bin}/commitguard version")
    assert_match version.to_s, shell_output("#{bin}/commitlint-rust --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint-rust --help")
  end
end
