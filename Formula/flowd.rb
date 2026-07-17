# Homebrew formula for flowd.
#
# Install from this repository (development / pre-release):
#   brew install --HEAD --formula Formula/flowd.rb
#
# Once a stable tag exists, fill in `url` / `sha256` below and publish to a tap.

class Flowd < Formula
  desc "Local-first terminal workflow automation for repeated file workflows"
  homepage "https://github.com/nickvd7/flowd"
  license "MIT"
  head "https://github.com/nickvd7/flowd.git", branch: "main"

  # Stable release block — after tagging, run:
  #   ./scripts/homebrew-sha256.sh 1.0.0
  # then uncomment and paste url/sha256/version below (and keep or drop `head`).
  # url "https://github.com/nickvd7/flowd/archive/refs/tags/v1.0.0.tar.gz"
  # sha256 "REPLACE_WITH_OUTPUT_FROM_scripts/homebrew-sha256.sh"
  # version "1.0.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix, "--path", "crates/flow-cli"
    system "cargo", "install", "--locked", "--root", prefix, "--path", "crates/flow-daemon"

    # `cargo install --root` places binaries under libexec/cargo/bin or bin/
    # depending on Homebrew/cargo layout; normalize into the formula bindir.
    bins = ["flowctl", "flow-cli", "flow-daemon"]
    bins.each do |name|
      candidate = [
        prefix/"bin"/name,
        prefix/"libexec/cargo/bin"/name,
        buildpath/"target/release"/name,
      ].find(&:exist?)
      next if candidate.nil?

      bin.install candidate => name unless (bin/name).exist?
    end

    ln_sf bin/"flow-cli", bin/"flowctl" if (bin/"flow-cli").exist? && !(bin/"flowctl").exist?
  end

  service do
    run [opt_bin/"flow-daemon"]
    keep_alive true
    working_dir var/"flowd"
    log_path var/"log/flowd.log"
    error_log_path var/"log/flowd.log"
  end

  test do
    assert_match(/flowctl|Usage|Commands/i, shell_output("#{bin}/flowctl --help"))
    assert_match(/flow-daemon|Usage|Commands/i, shell_output("#{bin}/flow-daemon --help"))
  end
end
