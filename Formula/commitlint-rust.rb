class CommitlintRust < Formula
  desc "Compatible combined lint and Git identity guard distribution"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-rust-aarch64-apple-darwin.tar.gz"
      sha256 "850d9e4d7ef33f43cf12d593ddc5812d275f9f1cf8152169e6d56ede38ab6ec6"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-rust-x86_64-apple-darwin.tar.gz"
      sha256 "0b2472fb2538f917611ff6136823dbfb68734538130766761bc7fea2ce664a44"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-rust-aarch64-unknown-linux-musl.tar.gz"
      sha256 "010418f69a55dd5d9ebeeb265e5de2cd1e0aa3657c6172ecf8ce49a784486fce"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-rust-x86_64-unknown-linux-musl.tar.gz"
      sha256 "f855971f4e6164a8fcb24f903270c45eb7d5abfa2634dfc292811b2972785f5b"
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
