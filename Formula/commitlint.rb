class Commitlint < Formula
  desc "Native Conventional Commits message checker"
  homepage "https://github.com/tkgstrator/commitlint-rust"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-only-aarch64-apple-darwin.tar.gz"
      sha256 "04737f83f1dfeef3f1190304c9bed3548135f0c8a319ee2f0619fd2e1956f077"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-only-x86_64-apple-darwin.tar.gz"
      sha256 "36e172f0afcc9a4f6bea976b1df1d03260bfca2784c4c1aa33d34b695938426d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-only-aarch64-unknown-linux-musl.tar.gz"
      sha256 "ccd3062b3af48a32af619d50ac2b0faf14ca22a4192c1492b472b277fb34d148"
    end
    on_intel do
      url "https://github.com/tkgstrator/commitlint-rust/releases/download/v0.2.0/commitlint-only-x86_64-unknown-linux-musl.tar.gz"
      sha256 "55fffd1be588f1632160aa0a6246acf201485a4c75e355d79c865cc95d51151e"
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
