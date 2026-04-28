# Formula template for a Homebrew tap.
#
# Expected publishing flow:
# 1. Copy this file to Formula/dotsync.rb inside your tap.
# 2. Replace SHA256 and the license once you choose it.
# 3. Publish a v0.1.0 release/tag in the main repository.

class Dotsync < Formula
  desc "CLI and library for applying and reversing dotfile sync"
  homepage "https://github.com/pookdeveloper/dotsync"
  url "https://github.com/pookdeveloper/dotsync/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SOURCE_TARBALL_SHA256"
  # license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix, "--path", "."
  end

  test do
    assert_match "dotsync", shell_output("#{bin}/dotsync --help 2>&1")
  end
end
