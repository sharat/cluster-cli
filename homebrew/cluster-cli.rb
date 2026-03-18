class ClusterCli < Formula
  desc "Fast, interactive terminal UI for monitoring Kubernetes cluster health"
  homepage "https://github.com/sharat/cluster-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-aarch64-apple-darwin.tar.gz"
      sha256 "c22f25bb884be312b35d8e4ac46b981546a2f8b093b7d0114fdc195809c6aeb4"
    else
      odie "cluster-cli only supports Apple Silicon (ARM64). Intel Macs are not supported."
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "31513c766045b6aba276a62f37d0ef0b5670b222b3662d2d032ffcb8b923cd94"
    end
  end

  depends_on "kubectl"

  def install
    bin.install "cluster"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cluster --version 2>&1 || true")
  end
end
