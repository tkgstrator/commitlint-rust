class CommitlintRust < Formula
  desc "Native Conventional Commits linting and gh identity guard"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.1.0/commitlint-rust-aarch64-apple-darwin.tar.gz"
      sha256 "5b793eea57d6d07e136408a9e82d6270115578be7f6c305e270197fcdf664ef8"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.1.0/commitlint-rust-x86_64-apple-darwin.tar.gz"
      sha256 "89979f9602db31a8359067e9b91899f374ae025b38e8e41d2bc08225c467fa48"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.1.0/commitlint-rust-aarch64-unknown-linux-musl.tar.gz"
      sha256 "ffb7981c1defeceadb6949b06a6d009a17ab83c1c889cebd57027b2dda4d4060"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.1.0/commitlint-rust-x86_64-unknown-linux-musl.tar.gz"
      sha256 "a00d1da6fd86f9ffda47dd1759d37cb26f6503c4e3216b0a7f12ce2288a9d0ee"
    end
  end

  depends_on "gh"
  depends_on "git"

  def install
    bin.install "gh-commit-guard"
    bin.install "commitlint" => "commitlint-rust"
  end

  def caveats
    <<~EOS
      To activate user-wide Git guards and Codex/Claude skills, run:
        gh-commit-guard install
      This changes user Git and shell/agent configuration. Reopen your shell afterward.
      Finish any active history-repair task before updating its installed guard.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gh-commit-guard version")
    assert_match version.to_s, shell_output("#{bin}/commitlint-rust --version")
    assert_match "Usage:", shell_output("#{bin}/commitlint-rust --help")
  end
end
